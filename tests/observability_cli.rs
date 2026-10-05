//! Structured-logging contract, checked against the real `forge` binary.
//!
//! A process gets one global `tracing` subscriber for its lifetime, and the
//! format is chosen from `FORGE_LOG_FORMAT` when it is installed, so these
//! tests run `forge` as a child process and parse its stderr. They pin:
//!
//! * `FORGE_LOG_FORMAT=json` emits one JSON object per line, with the keys
//!   log aggregators index on (issue #120);
//! * the HTTP server's `TraceLayer` emits the per-request
//!   `tower_http::trace::on_response` event with status and latency inside
//!   the `request` span (method, uri, request id), and a handler's
//!   `log.info` inherits that span (issue #119).

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value as Json;

fn forge() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forge"));
    // The contract under test is the default filter and the JSON layer;
    // don't inherit a developer's filter overrides.
    cmd.env_remove("FORGE_LOG")
        .env_remove("RUST_LOG")
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .env_remove("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
        .env("FORGE_LOG_FORMAT", "json")
        .env("NO_COLOR", "1");
    cmd
}

fn script(name: &str, source: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "forge_observability_{}_{}_{}.fg",
        name,
        std::process::id(),
        nanos
    ));
    std::fs::write(&path, source).expect("write script");
    path
}

/// Parse every non-empty stderr line as a JSON object, failing with the
/// offending line otherwise.
fn parse_json_lines(stderr: &str) -> Vec<Json> {
    stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: Json = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("stderr line is not JSON ({e}): {line:?}\n--- full stderr ---\n{stderr}")
            });
            assert!(
                value.is_object(),
                "stderr line is not a JSON object: {line}"
            );
            value
        })
        .collect()
}

fn find<'a>(events: &'a [Json], target: &str, message: Option<&str>) -> Option<&'a Json> {
    events.iter().find(|e| {
        e["target"] == target && message.is_none_or(|m| e["fields"]["message"].as_str() == Some(m))
    })
}

/// Keys every formatted event carries.
fn assert_common_keys(event: &Json) {
    for key in ["timestamp", "level", "target", "fields"] {
        assert!(event.get(key).is_some(), "event lacks `{key}`: {event}");
    }
    assert!(
        !event.to_string().contains('\u{1b}'),
        "JSON output must not contain ANSI escapes: {event}"
    );
}

#[test]
fn json_format_emits_parseable_user_log_events() {
    let path = script(
        "user_log",
        "log.info(\"hello from forge\")\n\
         log.warn(\"careful\", 42)\n\
         log.debug(\"filtered out by the default filter\")\n\
         say \"done\"\n",
    );
    let output = forge().arg("run").arg(&path).output().expect("run forge");
    let _ = std::fs::remove_file(&path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "forge failed:\n{stderr}");
    assert_eq!(stdout.trim(), "done", "log output must not reach stdout");

    let events = parse_json_lines(&stderr);
    let info = find(&events, "forge.user", Some("hello from forge"))
        .unwrap_or_else(|| panic!("no forge.user info event in:\n{stderr}"));
    assert_common_keys(info);
    assert_eq!(info["level"], "INFO");

    let warn = find(&events, "forge.user", Some("careful 42"))
        .unwrap_or_else(|| panic!("no forge.user warn event in:\n{stderr}"));
    assert_eq!(warn["level"], "WARN");

    assert!(
        find(
            &events,
            "forge.user",
            Some("filtered out by the default filter")
        )
        .is_none(),
        "debug events are below the default filter"
    );
}

fn pick_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

/// Kills the server child even when an assertion fails.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The default engine (the VM serves `@server` programs).
#[test]
fn server_emits_request_span_and_on_response_event_vm() {
    server_emits_request_span_and_on_response_event(&[]);
}

#[test]
fn server_emits_request_span_and_on_response_event_interpreter() {
    server_emits_request_span_and_on_response_event(&["--interp"]);
}

fn server_emits_request_span_and_on_response_event(engine_flags: &[&str]) {
    let port = pick_port();
    let path = script(
        "server",
        &format!(
            "@server(port: {port})\n\
             \n\
             @get(\"/ping\")\n\
             fn ping() -> Json {{\n\
                 log.info(\"handled ping\")\n\
                 return {{ ok: true }}\n\
             }}\n"
        ),
    );
    let mut child = forge()
        .args(engine_flags)
        .arg("run")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn forge server");
    let stderr = child.stderr.take().expect("piped stderr");
    let _child = KillOnDrop(child);

    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    {
        let lines = Arc::clone(&lines);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                lines.lock().unwrap_or_else(|p| p.into_inner()).push(line);
            }
        });
    }
    let captured = || lines.lock().unwrap_or_else(|p| p.into_inner()).join("\n");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .expect("client");
    let url = format!("http://127.0.0.1:{port}/ping");
    let request_id = "observability-test-req-1";
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let ok = client
            .get(&url)
            .header("X-Request-Id", request_id)
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        if ok {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server did not answer on {url}; stderr:\n{}",
            captured()
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // The response event is written after the response is produced; give
    // the server a moment to flush it.
    let deadline = Instant::now() + Duration::from_secs(10);
    let events = loop {
        let text = captured();
        let events = parse_json_lines(&text);
        let done = events.iter().any(|e| {
            e["target"] == "tower_http::trace::on_response" && e["span"]["request_id"] == request_id
        });
        if done {
            break events;
        }
        assert!(
            Instant::now() < deadline,
            "no tower_http::trace::on_response event for the request; stderr:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };

    let listening =
        find(&events, "forge.server", Some("Forge server listening")).expect("startup event");
    assert_common_keys(listening);
    assert_eq!(listening["fields"]["port"], port);

    let response = events
        .iter()
        .find(|e| {
            e["target"] == "tower_http::trace::on_response" && e["span"]["request_id"] == request_id
        })
        .expect("on_response event");
    assert_common_keys(response);
    assert_eq!(response["level"], "INFO");
    let status = &response["fields"]["status"];
    assert!(
        status == 200 || status == "200",
        "on_response status should be 200: {response}"
    );
    assert!(
        response["fields"].get("latency").is_some(),
        "on_response lacks latency: {response}"
    );
    let span = &response["span"];
    assert_eq!(span["name"], "request", "{response}");
    assert_eq!(span["method"], "GET", "{response}");
    assert_eq!(span["uri"], "/ping", "{response}");

    // The handler's user log runs inside the request span.
    let handled = events
        .iter()
        .find(|e| {
            e["target"] == "forge.user"
                && e["fields"]["message"] == "handled ping"
                && e["spans"]
                    .as_array()
                    .is_some_and(|spans| spans.iter().any(|s| s["request_id"] == request_id))
        })
        .unwrap_or_else(|| panic!("handler log.info lacks the request span; events: {events:?}"));
    let request_span = handled["spans"]
        .as_array()
        .and_then(|spans| spans.iter().find(|s| s["name"] == "request"))
        .expect("request span in handler event");
    assert_eq!(request_span["method"], "GET");
    assert_eq!(request_span["uri"], "/ping");

    let _ = std::fs::remove_file(&path);
}
