//! Outbound W3C trace-context propagation (issue #134).
//!
//! With OTel export active, an HTTP request made by Forge code carries a
//! `traceparent` whose trace id is the caller's trace and whose parent id
//! is the client span for that request. Runs in its own process because
//! the OTel provider and the tracing subscriber are process-global.
#![cfg(feature = "otel")]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use forge_lang::runtime::{client, tracing_init};
use opentelemetry::trace::TraceContextExt;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const UPSTREAM_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const UPSTREAM_SPAN: &str = "00f067aa0ba902b7";

/// Activate OTel export once for the whole binary (unreachable collector:
/// spans are batched and dropped, which is fine here).
fn otel() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .expect("ephemeral port");
        std::env::set_var(
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            format!("http://127.0.0.1:{port}"),
        );
        std::env::set_var("OTEL_BSP_EXPORT_TIMEOUT", "500");
        // The client's SSRF guard rejects loopback unless allowed.
        std::env::set_var("FORGE_HTTP_ALLOW_PRIVATE", "1");
        // Record this test's own spans alongside the client's.
        std::env::set_var("FORGE_LOG", "otel_propagation=info,forge_lang=info");
        // The exporter's gRPC channel must be built inside a Tokio runtime
        // that outlives it (the CLI uses its main runtime).
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("tokio runtime")
        });
        let _guard = runtime.enter();
        tracing_init::init_otel();
        assert!(
            tracing_init::otel_is_active(),
            "OTel export should be active"
        );
        tracing_init::init_subscriber();
    });
}

/// A one-shot HTTP server that reports the request headers it received.
fn header_capturing_server() -> (String, mpsc::Receiver<Vec<(String, String)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!(
        "http://127.0.0.1:{}/",
        listener.local_addr().expect("addr").port()
    );
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut headers = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
            }
        }
        let mut stream = stream;
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        );
        let _ = tx.send(headers);
    });
    (url, rx)
}

fn header(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

fn fetch_capturing(
    user_headers: Option<&std::collections::HashMap<String, String>>,
) -> Vec<(String, String)> {
    let (url, rx) = header_capturing_server();
    let response = client::fetch_blocking(&url, "GET", None, user_headers, Some(5), None, None)
        .expect("fetch succeeds");
    drop(response);
    rx.recv_timeout(Duration::from_secs(5))
        .expect("server saw the request")
}

#[test]
fn outbound_request_continues_the_callers_trace() {
    otel();
    // A span whose parent is an upstream traceparent, as the server's
    // TraceLayer sets up for an inbound request.
    let span = tracing::info_span!("request");
    let upstream = {
        let mut carrier = std::collections::HashMap::new();
        carrier.insert(
            "traceparent".to_string(),
            format!("00-{UPSTREAM_TRACE}-{UPSTREAM_SPAN}-01"),
        );
        opentelemetry::global::get_text_map_propagator(|p| p.extract(&carrier))
    };
    let _ = span.set_parent(upstream);

    let headers = span.in_scope(|| fetch_capturing(None));
    let traceparent = header(&headers, "traceparent").expect("traceparent sent");
    let parts: Vec<&str> = traceparent.split('-').collect();
    assert_eq!(parts.len(), 4, "{traceparent}");
    assert_eq!(parts[0], "00");
    assert_eq!(parts[1], UPSTREAM_TRACE, "same trace as the caller");
    assert_ne!(
        parts[2], UPSTREAM_SPAN,
        "parent is the client span, not upstream"
    );
    assert_ne!(parts[2], "0000000000000000");
    let request_span_id = span.context().span().span_context().span_id().to_string();
    assert_ne!(
        parts[2], request_span_id,
        "parent is the per-request client span, a child of the caller's span"
    );
    assert_eq!(parts[3], "01", "sampled flag follows the upstream decision");
}

#[test]
fn outbound_request_without_a_parent_starts_a_trace() {
    otel();
    let headers = fetch_capturing(None);
    let traceparent = header(&headers, "traceparent").expect("traceparent sent");
    let parts: Vec<&str> = traceparent.split('-').collect();
    assert_eq!(parts.len(), 4, "{traceparent}");
    assert_ne!(parts[1], "00000000000000000000000000000000");
    assert_ne!(parts[1], UPSTREAM_TRACE);
}

#[test]
fn explicit_traceparent_header_is_not_overwritten() {
    otel();
    let mine = format!("00-{UPSTREAM_TRACE}-{UPSTREAM_SPAN}-00");
    let mut user = std::collections::HashMap::new();
    user.insert("TraceParent".to_string(), mine.clone());
    let headers = fetch_capturing(Some(&user));
    let sent: Vec<&String> = headers
        .iter()
        .filter(|(k, _)| k == "traceparent")
        .map(|(_, v)| v)
        .collect();
    assert_eq!(sent, vec![&mine], "exactly the caller's header");
}
