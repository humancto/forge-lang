//! HTTP client failures are distinguishable (V2 finding 23): a request that
//! times out is E0036 (`e.type == "TimeoutError"`), a connection that cannot
//! be made is E0037 (`"ConnectionError"`), on both engines, and the
//! `timeout` option is in seconds (Int or Float).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const FORGE: &str = env!("CARGO_BIN_EXE_forge");

/// A server that answers every request after `delay`.
fn slow_server(delay: Duration) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer_late(stream, delay));
        }
    });
    port
}

fn answer_late(mut stream: TcpStream, delay: Duration) {
    let mut buf = [0u8; 4096];
    let _ = stream.read(&mut buf);
    std::thread::sleep(delay);
    let _ =
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
}

/// A loopback port with nothing listening on it.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("forge-http-errors-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn run(dir: &PathBuf, engine: Option<&str>, source: &str) -> (Output, Duration) {
    std::fs::write(dir.join("t.fg"), source).expect("write program");
    let mut cmd = Command::new(FORGE);
    cmd.args(engine)
        .args(["run", "t.fg"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .env("FORGE_HTTP_ALLOW_PRIVATE", "1")
        .env("NO_COLOR", "1");
    let start = Instant::now();
    let out = cmd.output().expect("run forge");
    (out, start.elapsed())
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

const ENGINES: [Option<&str>; 2] = [None, Some("--interp")];

#[test]
fn a_timeout_is_e0036_and_counts_seconds() {
    let port = slow_server(Duration::from_secs(3));
    let dir = scratch("timeout");
    for engine in ENGINES {
        let src = format!(
            "try {{\n    http.get(\"http://127.0.0.1:{port}/slow?token=secret\", {{ timeout: 1 }})\n}} catch e {{\n    println(e.code)\n    println(e.type)\n    println(e.message)\n}}\n"
        );
        let (out, elapsed) = run(&dir, engine, &src);
        assert!(out.status.success(), "{engine:?}: {out:?}");
        let text = stdout(&out);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "E0036", "{engine:?}: {text}");
        assert_eq!(lines[1], "TimeoutError", "{engine:?}: {text}");
        assert_eq!(
            lines[2],
            format!("http.get error: timed out after 1 second waiting for 127.0.0.1:{port}"),
            "{engine:?}"
        );
        assert!(
            !text.contains("secret"),
            "the URL's query is not echoed: {text}"
        );
        // One second, not milliseconds and not the 3 s the server takes.
        assert!(
            elapsed >= Duration::from_millis(900) && elapsed < Duration::from_millis(2900),
            "{engine:?}: {elapsed:?}"
        );

        let src = format!(
            "try {{\n    http.post(\"http://127.0.0.1:{port}/\", {{ body: \"x\", timeout: 0.25 }})\n}} catch e {{\n    println(e.code, e.message)\n}}\n"
        );
        let (out, _) = run(&dir, engine, &src);
        assert_eq!(
            stdout(&out).lines().next().unwrap_or(""),
            format!(
                "E0036 http.post error: timed out after 0.25 seconds waiting for 127.0.0.1:{port}"
            ),
            "{engine:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_slow_answer_within_the_timeout_succeeds() {
    let port = slow_server(Duration::from_millis(300));
    let dir = scratch("slow-ok");
    for engine in ENGINES {
        let src = format!(
            "let r = http.get(\"http://127.0.0.1:{port}/\", {{ timeout: 5 }})\nprintln(r.status, r.body)\n"
        );
        let (out, _) = run(&dir, engine, &src);
        assert!(out.status.success(), "{engine:?}: {out:?}");
        assert_eq!(stdout(&out), "200 ok\n", "{engine:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_connection_is_e0037() {
    let port = closed_port();
    let dir = scratch("refused");
    for engine in ENGINES {
        let src = format!(
            "try {{\n    http.get(\"http://127.0.0.1:{port}/\")\n}} catch e {{\n    println(e.code)\n    println(e.type)\n    println(e.message)\n}}\ntry {{\n    fetch(\"http://127.0.0.1:{port}/\")\n}} catch e {{\n    println(e.code)\n}}\n"
        );
        let (out, _) = run(&dir, engine, &src);
        assert!(out.status.success(), "{engine:?}: {out:?}");
        let text = stdout(&out);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "E0037", "{engine:?}: {text}");
        assert_eq!(lines[1], "ConnectionError", "{engine:?}: {text}");
        assert_eq!(
            lines[2],
            format!("http.get error: connection refused by 127.0.0.1:{port}"),
            "{engine:?}: {text}"
        );
        assert_eq!(lines.last().copied(), Some("E0037"), "{engine:?}: {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_invalid_timeout_is_an_argument_error() {
    let dir = scratch("bad-timeout");
    for engine in ENGINES {
        for bad in ["\"5\"", "-1", "true"] {
            let src = format!(
                "try {{\n    http.get(\"http://127.0.0.1:1/\", {{ timeout: {bad} }})\n}} catch e {{\n    println(e.code)\n}}\n"
            );
            let (out, _) = run(&dir, engine, &src);
            assert_eq!(stdout(&out), "E0015\n", "{engine:?} {bad}: {out:?}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
