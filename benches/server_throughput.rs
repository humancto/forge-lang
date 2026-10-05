//! End-to-end HTTP throughput of `forge run` on both serving engines.
//!
//! Boots the real `forge` binary on `examples/bench_server.fg` (trivial
//! handler) and `examples/bench_server_concurrent.fg` (CPU-bound handler),
//! once on the bytecode VM (the default) and once with `--interp`, drives
//! each with a closed-loop keep-alive load generator and prints requests per
//! second and latency percentiles.
//!
//! ```text
//! cargo bench --bench server_throughput
//! FORGE_BENCH_SECS=10 FORGE_BENCH_CONCURRENCY=64 cargo bench --bench server_throughput
//! ```
//!
//! Each example is copied to a temporary file with its port replaced by a
//! free one, so a busy :9090 does not matter. Numbers depend on the machine
//! and on other load; compare engines within one run.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Scenario {
    label: &'static str,
    example: &'static str,
    path: &'static str,
    /// Concurrency multiplier relative to `FORGE_BENCH_CONCURRENCY`
    /// (CPU-bound handlers saturate the cores with fewer clients).
    concurrency_divisor: usize,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        label: "trivial  GET /ping",
        example: "examples/bench_server.fg",
        path: "/ping",
        concurrency_divisor: 1,
    },
    Scenario {
        label: "cpu-bound GET /cpu",
        example: "examples/bench_server_concurrent.fg",
        path: "/cpu",
        concurrency_divisor: 4,
    },
];

const ENGINES: &[(&str, &[&str])] = &[("vm", &[]), ("interpreter", &["--interp"])];

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("bind an ephemeral port")
}

/// Copy `example` with its `@server(port: ...)` replaced by `port`.
fn example_on_port(root: &Path, example: &str, port: u16) -> PathBuf {
    let source = std::fs::read_to_string(root.join(example)).expect("read example");
    let start = source
        .find("@server(port: ")
        .expect("example has @server(port: N)");
    let digits_start = start + "@server(port: ".len();
    let digits_len = source[digits_start..]
        .find(|c: char| !c.is_ascii_digit())
        .expect("port number ends");
    let rewritten = format!(
        "{}{}{}",
        &source[..digits_start],
        port,
        &source[digits_start + digits_len..]
    );
    let file = std::env::temp_dir().join(format!(
        "forge_bench_{}_{}",
        port,
        Path::new(example)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("server.fg")
    ));
    std::fs::write(&file, rewritten).expect("write example copy");
    file
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server(forge: &Path, flags: &[&str], program: &Path, port: u16) -> Server {
    let child = Command::new(forge)
        .args(flags)
        .arg("run")
        .arg(program)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("FORGE_LOG", "warn")
        .spawn()
        .expect("spawn forge");
    let server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    let url = format!("http://127.0.0.1:{}/ping", port);
    let client = reqwest::blocking::Client::new();
    while Instant::now() < deadline {
        if client
            .get(&url)
            .send()
            .is_ok_and(|r| r.status().is_success())
        {
            return server;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("{} {:?} did not start serving", program.display(), flags);
}

struct Report {
    requests: usize,
    errors: usize,
    elapsed: Duration,
    latencies: Vec<Duration>,
}

impl Report {
    fn percentile(&self, p: f64) -> Duration {
        if self.latencies.is_empty() {
            return Duration::ZERO;
        }
        let idx = ((self.latencies.len() as f64 - 1.0) * p).round() as usize;
        self.latencies[idx]
    }
}

/// Closed loop: `concurrency` clients each send the next request as soon as
/// the previous response arrives, for `duration` (after a short warm-up).
fn load(url: &str, concurrency: usize, duration: Duration) -> Report {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("load runtime");
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(concurrency)
            .build()
            .expect("client");
        let url: Arc<str> = Arc::from(url);
        // Warm up connections (and the engines' one-time paths).
        let warm_until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < warm_until {
            let _ = client.get(&*url).send().await.map(|r| r.bytes());
        }
        let start = Instant::now();
        let stop = start + duration;
        let tasks: Vec<_> = (0..concurrency)
            .map(|_| {
                let client = client.clone();
                let url = Arc::clone(&url);
                tokio::spawn(async move {
                    let mut latencies = Vec::new();
                    let mut errors = 0;
                    while Instant::now() < stop {
                        let t0 = Instant::now();
                        match client.get(&*url).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                let _ = resp.bytes().await;
                                latencies.push(t0.elapsed());
                            }
                            _ => errors += 1,
                        }
                    }
                    (latencies, errors)
                })
            })
            .collect();
        let mut latencies = Vec::new();
        let mut errors = 0;
        for task in tasks {
            let (l, e) = task.await.expect("load task");
            latencies.extend(l);
            errors += e;
        }
        let elapsed = start.elapsed();
        latencies.sort();
        Report {
            requests: latencies.len(),
            errors,
            elapsed,
            latencies,
        }
    })
}

fn main() {
    // `cargo bench` passes `--bench`; there are no other options.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let forge = PathBuf::from(env!("CARGO_BIN_EXE_forge"));
    let secs = env_usize("FORGE_BENCH_SECS", 5);
    let concurrency = env_usize("FORGE_BENCH_CONCURRENCY", 32);

    println!(
        "forge server throughput ({}s per run, {} cores, binary {})",
        secs,
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        forge.display()
    );
    println!(
        "{:<20} {:<12} {:>5} {:>10} {:>9} {:>9} {:>9} {:>7}",
        "scenario", "engine", "conc", "req/s", "p50", "p99", "max", "errors"
    );
    for scenario in SCENARIOS {
        let conc = (concurrency / scenario.concurrency_divisor).max(1);
        for (engine, flags) in ENGINES {
            let port = free_port();
            let program = example_on_port(&root, scenario.example, port);
            let server = start_server(&forge, flags, &program, port);
            let url = format!("http://127.0.0.1:{}{}", port, scenario.path);
            let report = load(&url, conc, Duration::from_secs(secs as u64));
            drop(server);
            let _ = std::fs::remove_file(&program);
            println!(
                "{:<20} {:<12} {:>5} {:>10.0} {:>9.2?} {:>9.2?} {:>9.2?} {:>7}",
                scenario.label,
                engine,
                conc,
                report.requests as f64 / report.elapsed.as_secs_f64(),
                report.percentile(0.50),
                report.percentile(0.99),
                report.latencies.last().copied().unwrap_or_default(),
                report.errors
            );
        }
    }
}
