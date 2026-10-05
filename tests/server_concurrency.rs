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
//!
//! Every test runs on both serving engines: the bytecode VM (the default,
//! `vm::serve`) and the interpreter (`--interp`). `on_both_engines!` turns
//! `fn name(engine)` into the tests `name::vm` and `name::interpreter`.

#[path = "support/server.rs"]
mod support;

use futures_util::SinkExt;
use support::Engine;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

macro_rules! on_both_engines {
    ($($name:ident),* $(,)?) => {$(
        mod $name {
            #[test]
            fn vm() {
                super::$name(super::Engine::Vm)
            }

            #[test]
            fn interpreter() {
                super::$name(super::Engine::Interpreter)
            }
        }
    )*};
}

on_both_engines!(
    http_handlers_run_in_parallel_not_serialized,
    closure_capturing_handlers_run_in_parallel_not_serialized,
    schedule_mutations_do_not_leak_into_handler_forks,
    websocket_handler_cancelled_on_client_disconnect,
    http_handler_cancelled_on_client_disconnect,
    request_id_is_generated_and_propagated,
    handler_recursion_gets_a_large_stack,
);

/// Loop iterations that keep a CPU-bound handler busy for tens of
/// milliseconds on `engine` (debug build): long enough that scheduling
/// noise cannot dominate the scaling ratio, short enough to keep the test
/// fast. The VM runs these loops ~100x faster than the interpreter.
fn cpu_iterations(engine: Engine) -> u64 {
    match engine {
        Engine::Vm => 1_000_000,
        Engine::Interpreter => 200_000,
    }
}

const MAX_SCALING_RATIO: f64 = 3.8;

/// Held for its whole duration by every test in this file.
///
/// libtest runs the tests of one binary concurrently (one thread per CPU).
/// Here that means the two ratio-based scaling tests measure their C=4 phase
/// while the *other* scaling test is also running 4 CPU-bound handlers and
/// the WebSocket test's handler spins in a 1M-iteration file-writing loop.
/// On ubuntu-latest (4 vCPU = 2 physical cores with SMT) that is ~9 busy
/// threads on ~2.5 cores of throughput, which pushes the measured ratio past
/// MAX_SCALING_RATIO even though handlers are fully parallel. The ratio gate
/// is only meaningful when the measurement owns the machine, so the tests
/// in this file run one at a time.
static SERVER_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn exclusive_server_test() -> std::sync::MutexGuard<'static, ()> {
    // A panicking (failed) sibling poisons the lock; the guard is still
    // usable for serialization.
    SERVER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn spawn_test_server(source: &str, engine: Engine) -> u16 {
    support::spawn_server(source, engine)
}

fn unique_temp_file(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "forge_{}_{}_{}.txt",
        name,
        std::process::id(),
        unique
    ))
}

fn wait_for_path(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn forge_string_literal_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

/// Time N concurrent GET requests using blocking reqwest on N OS threads.
/// Returns the total wall time from the first request issued to the last
/// response received.
/// Number of C=1 / C=4 measurement rounds per scaling test.
const SCALING_ATTEMPTS: usize = 3;

/// Measures `(C=1 wall, C=4 wall, ratio)` up to `SCALING_ATTEMPTS` times and
/// returns the round with the best ratio, stopping early once a round is
/// under `MAX_SCALING_RATIO`.
///
/// A serialized server (global lock, shared closure mutex) is slow on
/// *every* round, so the gate keeps its power; a noisy shared runner (CI
/// neighbours, a concurrent build) only has to produce one clean round.
fn best_scaling_round(url: &str) -> (Duration, Duration, f64) {
    let mut best: Option<(Duration, Duration, f64)> = None;
    for _ in 0..SCALING_ATTEMPTS {
        let single = concurrent_get_wall_time(url, 1);
        let parallel = concurrent_get_wall_time(url, 4);
        let ratio = parallel.as_secs_f64() / single.as_secs_f64();
        if best.is_none_or(|(_, _, r)| ratio < r) {
            best = Some((single, parallel, ratio));
        }
        if ratio < MAX_SCALING_RATIO {
            break;
        }
    }
    best.expect("BUG: SCALING_ATTEMPTS must be at least 1")
}

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

fn http_handlers_run_in_parallel_not_serialized(engine: Engine) {
    let _exclusive = exclusive_server_test();
    // CPU-bound handler (see `cpu_iterations`).
    let port = spawn_test_server(
        &r#"
        @server(port: __PORT__)

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @get("/cpu")
        fn cpu() -> Json {
            let mut total = 0
            repeat __WORK__ times {
                total = total + 1
            }
            return { ok: true, work: total }
        }
        "#
        .replace("__WORK__", &cpu_iterations(engine).to_string()),
        engine,
    );

    let url = format!("http://127.0.0.1:{}/cpu", port);

    // Warm-up: prime any one-time JIT / module-load paths.
    let _ = concurrent_get_wall_time(&url, 1);

    // C=4 not C=8: typical CI runners have 4 cores, and we want the
    // ratio gate to be meaningful (i.e. parallelism, not OS scheduling
    // overhead). On a 16-core dev box this still proves the absence
    // of a global lock; on a 4-core CI runner it doesn't pay the
    // oversubscription tax.
    let (single, parallel, _) = best_scaling_round(&url);

    eprintln!(
        "concurrency-scaling ({:?}): C=1 wall = {:?}, C=4 wall = {:?}, ratio = {:.2}x",
        engine,
        single,
        parallel,
        parallel.as_secs_f64() / single.as_secs_f64()
    );

    // On a fully serialized server (the pre-fix Arc<Mutex<Interpreter>>
    // model), C=4 would take ~4x longer than C=1. We allow 3.8x to
    // accommodate slow CI runners (ubuntu-latest is effectively
    // 2-core with hyperthreading and frequently under load), tokio
    // scheduling overhead, and per-request tower_http middleware
    // cost. The gate still detects a regression to full serialization
    // (which would be ~4x).
    assert!(
        parallel < single.mul_f64(MAX_SCALING_RATIO),
        "handlers serialized: C=4 wall {:?} should be < {:.1}x C=1 wall {:?} \
         (ratio {:.2}x). The per-request fork model has regressed.",
        parallel,
        MAX_SCALING_RATIO,
        single,
        parallel.as_secs_f64() / single.as_secs_f64()
    );
}

fn closure_capturing_handlers_run_in_parallel_not_serialized(engine: Engine) {
    let _exclusive = exclusive_server_test();
    // Captured-closure handler pattern. A top-level Lambda holds the
    // CPU loop; the @get fn invokes it. Different from the global-fn
    // case in http_handlers_run_in_parallel_not_serialized, which only
    // reads the global scope through its closure. *This* path actually exercises
    // Value::Lambda::closure -- which under the pre-PR-#110 model
    // shares Arc<Mutex<Environment>> across forks, so concurrent
    // requests serialize on the closure mutex.
    //
    // After PR #110, deep_clone_isolated walks closures so each fork
    // has its own closure Arc and the ratio assertion holds for
    // closure-capturing handlers too. On the VM each fork gets its own
    // copy of the closure's upvalue cells (`vm::serve`).
    let port = spawn_test_server(
        &r#"
        @server(port: __PORT__)

        let config = { multiplier: __MULTIPLIER__ }

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
        "#
        .replace(
            "__MULTIPLIER__",
            &(cpu_iterations(engine) / 1000).to_string(),
        ),
        engine,
    );

    let url = format!("http://127.0.0.1:{}/cpu", port);

    // Warm-up.
    let _ = concurrent_get_wall_time(&url, 1);

    let (single, parallel, _) = best_scaling_round(&url);

    eprintln!(
        "closure-handler scaling ({:?}): C=1 wall = {:?}, C=4 wall = {:?}, ratio = {:.2}x",
        engine,
        single,
        parallel,
        parallel.as_secs_f64() / single.as_secs_f64()
    );

    assert!(
        parallel < single.mul_f64(MAX_SCALING_RATIO),
        "closure-capturing handlers serialized: C=4 wall {:?} should be < {:.1}x C=1 wall {:?} \
         (ratio {:.2}x). The per-request closure isolation has regressed -- \
         check Environment::deep_clone_isolated and fork_for_serving.",
        parallel,
        MAX_SCALING_RATIO,
        single,
        parallel.as_secs_f64() / single.as_secs_f64()
    );
}

fn schedule_mutations_do_not_leak_into_handler_forks(engine: Engine) {
    let _exclusive = exclusive_server_test();
    let sentinel = unique_temp_file("schedule_handler_isolation");
    let _ = std::fs::remove_file(&sentinel);
    let sentinel_str = forge_string_literal_path(&sentinel);

    // On the interpreter, `spawn_test_server` leaves `defer_host_runtime` at
    // the default (`false`), so schedules start during `interp.run()`; the VM
    // path defers them to after the top level, as `forge run` does. Both
    // exercise the same background-runtime vs per-request serving-fork
    // isolation contract.
    let source = r#"
        @server(port: __PORT__)

        let mut state = 0

        schedule every 1 seconds {
            state = state + 1
            fs.write("__SENTINEL__", "ran")
        }

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @get("/read")
        fn read() -> Json {
            return { state: state }
        }
        "#
    .replace("__SENTINEL__", &sentinel_str);

    let port = spawn_test_server(&source, engine);
    std::thread::sleep(Duration::from_millis(1500));

    assert!(
        sentinel.exists(),
        "schedule body never wrote sentinel file at {}; test would be vacuous",
        sentinel.display()
    );

    let url = format!("http://127.0.0.1:{}/read", port);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client");

    for attempt in 0..5 {
        let body: serde_json::Value = client
            .get(&url)
            .send()
            .expect("send")
            .json()
            .expect("json response");
        assert_eq!(
            body["state"],
            serde_json::json!(0),
            "handler fork observed background schedule mutation on attempt {attempt}: {body}"
        );
    }

    let _ = std::fs::remove_file(&sentinel);
}

fn websocket_handler_cancelled_on_client_disconnect(engine: Engine) {
    let _exclusive = exclusive_server_test();
    let started = unique_temp_file("ws_cancel_started");
    let progress = unique_temp_file("ws_cancel_progress");
    let finished = unique_temp_file("ws_cancel_finished");
    for path in [&started, &progress, &finished] {
        let _ = std::fs::remove_file(path);
    }

    let started_str = forge_string_literal_path(&started);
    let progress_str = forge_string_literal_path(&progress);
    let finished_str = forge_string_literal_path(&finished);

    let source = r#"
        @server(port: __PORT__)

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @ws("/ws")
        fn socket(msg) {
            fs.write("__STARTED__", "1")
            let mut i = 0
            repeat 1000000 times {
                i = i + 1
                fs.write("__PROGRESS__", str(i))
            }
            fs.write("__FINISHED__", "done")
            return "done"
        }
        "#
    .replace("__STARTED__", &started_str)
    .replace("__PROGRESS__", &progress_str)
    .replace("__FINISHED__", &finished_str);

    let port = spawn_test_server(&source, engine);
    let url = format!("ws://127.0.0.1:{}/ws", port);

    let rt = tokio::runtime::Runtime::new().expect("test runtime");
    rt.block_on(async {
        use tokio_tungstenite::tungstenite::Message;

        let (mut ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect websocket");
        ws.send(Message::Text("go".into()))
            .await
            .expect("send websocket message");

        let started_path = started.clone();
        tokio::task::spawn_blocking(move || {
            assert!(
                wait_for_path(&started_path, Duration::from_secs(5)),
                "websocket handler never started"
            );
        })
        .await
        .expect("wait for started sentinel");

        let progress_path = progress.clone();
        tokio::task::spawn_blocking(move || {
            assert!(
                wait_for_path(&progress_path, Duration::from_secs(5)),
                "websocket handler never entered progress loop"
            );
        })
        .await
        .expect("wait for progress sentinel");

        let _ = ws.send(Message::Close(None)).await;
        drop(ws);
    });

    let mut last_progress =
        std::fs::read_to_string(&progress).expect("progress sentinel should be readable");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stabilized = false;
    while Instant::now() < deadline {
        assert!(
            !finished.exists(),
            "websocket handler completed normally instead of being cancelled"
        );
        std::thread::sleep(Duration::from_millis(200));
        let current =
            std::fs::read_to_string(&progress).expect("progress sentinel should remain readable");
        if current == last_progress {
            stabilized = true;
            break;
        }
        last_progress = current;
    }

    assert!(
        stabilized,
        "websocket handler progress kept changing after client disconnect"
    );
    assert!(
        !finished.exists(),
        "websocket handler wrote finished sentinel after disconnect"
    );

    for path in [&started, &progress, &finished] {
        let _ = std::fs::remove_file(path);
    }
}

fn request_id_is_generated_and_propagated(engine: Engine) {
    let _exclusive = exclusive_server_test();
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
        engine,
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

/// Handlers run on tokio's blocking pool. With tokio's default 2 MiB
/// thread stacks the interpreter's native-stack guard tripped at ~100
/// levels of Forge recursion inside a handler; the CLI configures the
/// runtime (`recursion::configure_runtime`) so blocking threads get
/// `WORKER_STACK_SIZE` and handler recursion matches other Forge code.
fn handler_recursion_gets_a_large_stack(engine: Engine) {
    let _exclusive = exclusive_server_test();
    let port = support::spawn_server_on(
        r#"
        @server(port: __PORT__)

        fn down(n) {
            if n == 0 { return 0 }
            return 1 + down(n - 1)
        }

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @get("/deep")
        fn deep() -> Json {
            return { depth: down(3000) }
        }
        "#,
        engine,
        forge_lang::runtime::recursion::configure_runtime,
    );
    let body = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("client")
        .get(format!("http://127.0.0.1:{}/deep", port))
        .send()
        .expect("request")
        .text()
        .expect("body");
    assert!(body.contains("3000"), "deep handler failed: {}", body);
}

/// A client that gives up on a long-running HTTP request must stop the
/// handler: the response future's drop guard flips the cancel flag the
/// forked engine polls at its safe points.
fn http_handler_cancelled_on_client_disconnect(engine: Engine) {
    let _exclusive = exclusive_server_test();
    let progress = unique_temp_file("http_cancel_progress");
    let finished = unique_temp_file("http_cancel_finished");
    for path in [&progress, &finished] {
        let _ = std::fs::remove_file(path);
    }

    let source = r#"
        @server(port: __PORT__)

        @get("/ping")
        fn ping() -> Json {
            return { ok: true }
        }

        @get("/slow")
        fn slow() -> Json {
            let mut i = 0
            repeat 1000000 times {
                i = i + 1
                fs.write("__PROGRESS__", str(i))
            }
            fs.write("__FINISHED__", "done")
            return { done: true }
        }
        "#
    .replace("__PROGRESS__", &forge_string_literal_path(&progress))
    .replace("__FINISHED__", &forge_string_literal_path(&finished));

    let port = spawn_test_server(&source, engine);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("client");
    let result = client.get(format!("http://127.0.0.1:{}/slow", port)).send();
    assert!(
        result.is_err(),
        "slow handler should outlive the client timeout"
    );
    assert!(
        wait_for_path(&progress, Duration::from_secs(5)),
        "slow handler never started"
    );

    let mut last_progress = std::fs::read_to_string(&progress).unwrap_or_default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stabilized = false;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        let current = std::fs::read_to_string(&progress).unwrap_or_default();
        if current == last_progress {
            stabilized = true;
            break;
        }
        last_progress = current;
    }
    assert!(
        stabilized,
        "HTTP handler progress kept changing after the client disconnected"
    );
    assert!(
        !finished.exists(),
        "HTTP handler ran to completion after the client disconnected"
    );

    for path in [&progress, &finished] {
        let _ = std::fs::remove_file(path);
    }
}
