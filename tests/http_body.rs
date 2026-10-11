//! `http.post(url, { body })`: a String body is sent as is (Content-Type
//! application/json when it parses as JSON, text/plain otherwise); any
//! other value is encoded as JSON. `body: json.stringify(x)` must not be
//! encoded twice.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

/// A one-shot server that reports the Content-Type and body it received.
fn capturing_server() -> (String, mpsc::Receiver<(String, String)>) {
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
        let (mut content_type, mut length) = (String::new(), 0usize);
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
                match name.trim().to_ascii_lowercase().as_str() {
                    "content-type" => content_type = value.trim().to_string(),
                    "content-length" => length = value.trim().parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
        let mut body = vec![0; length];
        let _ = reader.read_exact(&mut body);
        let mut stream = stream;
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        );
        let _ = tx.send((content_type, String::from_utf8_lossy(&body).into_owned()));
    });
    (url, rx)
}

fn post(body_expr: &str) -> (String, String) {
    let (url, rx) = capturing_server();
    let code = format!("let r = http.post(\"{url}\", {{ body: {body_expr} }})\nprintln(r.status)");
    let out = Command::new(env!("CARGO_BIN_EXE_forge"))
        .env("FORGE_HTTP_ALLOW_PRIVATE", "1")
        .args(["-e", &code])
        .output()
        .expect("run forge");
    assert!(out.status.success(), "{:?}", out);
    rx.recv_timeout(Duration::from_secs(10))
        .expect("server saw the request")
}

#[test]
fn string_bodies_are_sent_as_is() {
    let (ct, body) = post("json.stringify({ a: 1 })");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("encoded once");
    assert_eq!(parsed, serde_json::json!({ "a": 1 }));
    assert!(ct.starts_with("application/json"), "{ct}");

    let (ct, body) = post("\"hello\"");
    assert_eq!(body, "hello");
    assert!(ct.starts_with("text/plain"), "{ct}");
}

#[test]
fn other_bodies_are_encoded_as_json() {
    let (ct, body) = post("{ a: 1 }");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json body");
    assert_eq!(parsed, serde_json::json!({ "a": 1 }));
    assert!(ct.starts_with("application/json"), "{ct}");
}
