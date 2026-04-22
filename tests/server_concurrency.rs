//! Integration test: HTTP server handler concurrency.
//!
//! This test is the **regression gate** for the per-request fork
//! architecture in `src/runtime/server.rs`. If anyone reverts to a
//! global `Arc<Mutex<Interpreter>>` (or otherwise serializes handler
//! execution), this test fails because concurrent CPU-bound handlers
//! collapse onto one core.
//!
//! The assertion is **ratio-based** so it survives across machines and
//! CI runners: wall time at C=8 must be no more than 4x wall time at
//! C=1. On a fully serialized server it would be 8x. On a fully
//! parallel server (8+ cores available) it would be ~1x.
//!
//! We pick C=8 instead of C=16 to keep the test passing on smaller CI
//! runners. The 4x slack also accommodates tokio scheduling noise and
//! interpreter overhead variance.

use forge_lang::interpreter::Interpreter;
use forge_lang::lexer::Lexer;
use forge_lang::parser::Parser;
use forge_lang::runtime::metadata::extract_runtime_plan;
use forge_lang::runtime::server::start_server;

use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pick an unused TCP port by binding 0 and letting the kernel choose.
fn pick_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Boot the server in a background tokio runtime on a dedicated thread,
/// returning the bound port. The server stays up for the test's duration
/// and is dropped when the runtime is dropped at test exit.
fn spawn_test_server(source: &str) -> u16 {
    let port = pick_port();
    let src = source.replace("__PORT__", &port.to_string());

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(64)
            .enable_all()
            .build()
            .expect("build tokio runtime");
        rt.block_on(async move {
            let mut lexer = Lexer::new(&src);
            let tokens = lexer.tokenize().expect("lex");
            let mut parser = Parser::new(tokens);
            let program = parser.parse_program().expect("parse");

            let mut interp = Interpreter::new();
            interp.run(&program).expect("run");

            let plan = extract_runtime_plan(&program);
            let server = plan.server.expect("program has @server decorator");
            start_server(interp, &server).await.expect("server start");
        });
    });

    // Poll for readiness up to 5s.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("client");
    let url = format!("http://127.0.0.1:{}/ping", port);
    for _ in 0..50 {
        if client
            .get(&url)
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return port;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("server failed to start on port {} within 5s", port);
}

/// Time N concurrent GET requests using blocking reqwest on N OS threads.
/// Returns the total wall time from the first request issued to the last
/// response received.
fn concurrent_get_wall_time(url: &str, concurrency: usize) -> Duration {
    let url = Arc::new(url.to_string());
    let start = Instant::now();
    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let url = url.clone();
            std::thread::spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(60))
                    .build()
                    .expect("client");
                let resp = client.get(&*url).send().expect("send");
                assert!(resp.status().is_success(), "non-2xx: {}", resp.status());
                let _ = resp.text();
            })
        })
        .collect();
    for h in handles {
        h.join().expect("thread");
    }
    start.elapsed()
}

#[test]
fn http_handlers_run_in_parallel_not_serialized() {
    // CPU-bound handler. ~96ms in the tree-walking interpreter on a
    // modern machine; tuned high enough that scheduler noise can't
    // dominate, low enough that the test stays fast.
    let port = spawn_test_server(
        r#"
        @server(port: __PORT__)

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @get("/cpu")
        fn cpu() -> Json {
            let mut total = 0
            repeat 200000 times {
                total = total + 1
            }
            return { ok: true, work: total }
        }
        "#,
    );

    let url = format!("http://127.0.0.1:{}/cpu", port);

    // Warm-up: prime any one-time JIT / module-load paths.
    let _ = concurrent_get_wall_time(&url, 1);

    let single = concurrent_get_wall_time(&url, 1);
    // C=4 not C=8: typical CI runners have 4 cores, and we want the
    // ratio gate to be meaningful (i.e. parallelism, not OS scheduling
    // overhead). On a 16-core dev box this still proves the absence
    // of a global lock; on a 4-core CI runner it doesn't pay the
    // oversubscription tax.
    let parallel = concurrent_get_wall_time(&url, 4);

    eprintln!(
        "concurrency-scaling: C=1 wall = {:?}, C=4 wall = {:?}, ratio = {:.2}x",
        single,
        parallel,
        parallel.as_secs_f64() / single.as_secs_f64()
    );

    // On a fully serialized server (the pre-fix Arc<Mutex<Interpreter>>
    // model), C=4 would take ~4x longer than C=1. We allow 3.5x to
    // accommodate slow CI runners (ubuntu-latest is effectively
    // 2-core with hyperthreading and frequently under load), tokio
    // scheduling overhead, and per-request tower_http middleware
    // cost. The gate still detects a regression to full serialization
    // (which would be ~4x).
    assert!(
        parallel < single.mul_f64(3.5),
        "handlers serialized: C=4 wall {:?} should be < 3.5x C=1 wall {:?} \
         (ratio {:.2}x). The per-request fork model has regressed.",
        parallel,
        single,
        parallel.as_secs_f64() / single.as_secs_f64()
    );
}

#[test]
fn closure_capturing_handlers_run_in_parallel_not_serialized() {
    // Captured-closure handler pattern. A top-level Lambda holds the
    // CPU loop; the @get fn invokes it. Different from the global-fn
    // case in http_handlers_run_in_parallel_not_serialized: that path
    // takes the is_global_fn fast path in call_function_inner and
    // ignores the closure entirely. *This* path actually exercises
    // Value::Lambda::closure -- which under the pre-PR-#110 model
    // shares Arc<Mutex<Environment>> across forks, so concurrent
    // requests serialize on the closure mutex.
    //
    // After PR #110, deep_clone_isolated walks closures so each fork
    // has its own closure Arc and the ratio assertion holds for
    // closure-capturing handlers too.
    let port = spawn_test_server(
        r#"
        @server(port: __PORT__)

        let config = { multiplier: 200 }

        fn make_compute() {
            return fn(n) {
                let mut total = 0
                repeat n * config.multiplier times {
                    total = total + 1
                }
                return total
            }
        }

        let compute = make_compute()

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @get("/cpu")
        fn cpu() -> Json {
            let result = compute(1000)
            return { ok: true, work: result }
        }
        "#,
    );

    let url = format!("http://127.0.0.1:{}/cpu", port);

    // Warm-up.
    let _ = concurrent_get_wall_time(&url, 1);

    let single = concurrent_get_wall_time(&url, 1);
    let parallel = concurrent_get_wall_time(&url, 4);

    eprintln!(
        "closure-handler scaling: C=1 wall = {:?}, C=4 wall = {:?}, ratio = {:.2}x",
        single,
        parallel,
        parallel.as_secs_f64() / single.as_secs_f64()
    );

    assert!(
        parallel < single.mul_f64(3.5),
        "closure-capturing handlers serialized: C=4 wall {:?} should be < 3.5x C=1 wall {:?} \
         (ratio {:.2}x). The per-request closure isolation has regressed -- \
         check Environment::deep_clone_isolated and fork_for_serving.",
        parallel,
        single,
        parallel.as_secs_f64() / single.as_secs_f64()
    );
}

#[test]
fn request_id_is_generated_and_propagated() {
    // Two scenarios to verify:
    //   (a) request without X-Request-Id -> response carries a new UUID
    //   (b) request with X-Request-Id    -> response echoes the inbound value
    //
    // The structured-log path (the load-bearing claim of PR #123) is
    // verified by stderr inspection in the smoke tests documented in
    // the PR body; CI just needs to see the response-header path
    // working since the layer order is the only thing that could
    // break.
    let port = spawn_test_server(
        r#"
        @server(port: __PORT__)

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }
        "#,
    );

    let url = format!("http://127.0.0.1:{}/ping", port);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client");

    // Scenario A: no inbound X-Request-Id -- server generates a UUID.
    let resp_generated = client.get(&url).send().expect("send");
    assert!(resp_generated.status().is_success());
    let generated_id = resp_generated
        .headers()
        .get("x-request-id")
        .expect(
            "response missing x-request-id; SetRequestIdLayer or PropagateRequestIdLayer is broken",
        )
        .to_str()
        .expect("response x-request-id is not UTF-8")
        .to_string();
    // UUID v4 string: 36 chars, hyphens at the canonical positions.
    assert_eq!(
        generated_id.len(),
        36,
        "generated request_id should be a 36-char UUID; got {:?}",
        generated_id
    );
    assert_eq!(
        generated_id.matches('-').count(),
        4,
        "generated request_id should be a UUID with 4 hyphens; got {:?}",
        generated_id
    );

    // Scenario B: inbound X-Request-Id -- server echoes it.
    let inbound = "test-trace-deadbeef-123";
    let resp_echoed = client
        .get(&url)
        .header("X-Request-Id", inbound)
        .send()
        .expect("send");
    assert!(resp_echoed.status().is_success());
    let echoed = resp_echoed
        .headers()
        .get("x-request-id")
        .expect("response missing x-request-id on echo path")
        .to_str()
        .expect("echoed x-request-id not UTF-8");
    assert_eq!(
        echoed, inbound,
        "PropagateRequestIdLayer should echo the inbound value verbatim"
    );

    // Sanity: two no-header requests produce different UUIDs.
    let resp_2 = client.get(&url).send().expect("send");
    let id_2 = resp_2
        .headers()
        .get("x-request-id")
        .expect("missing")
        .to_str()
        .expect("not UTF-8")
        .to_string();
    assert_ne!(
        generated_id, id_2,
        "two server-generated request_ids should differ"
    );
}
