//! Engine parity for decorator-driven HTTP servers.
//!
//! Boots the same `@server` program on the bytecode VM (the default
//! serving engine) and on the interpreter (`--interp`), sends both the same
//! requests, and requires identical responses: status, JSON body and
//! content type. Covers routing (params, query, JSON bodies, every method),
//! 404/405/body rejections, handler errors (500s), Forge panics,
//! per-request isolation of globals and captured closures, deep
//! recursion, and WebSocket echo. Cancellation on client disconnect is
//! checked for both engines in `tests/server_concurrency.rs`.

#[path = "support/server.rs"]
mod support;

use std::time::Duration;

use support::{spawn_server_on, Engine};

const PROGRAM: &str = r#"
@server(port: __PORT__)

let mut hits = 0
let mut log = []
let config = { greeting: "hello", tags: ["a", "b"] }

fn make_counter() {
    let mut n = 0
    return fn() {
        n = n + 1
        return n
    }
}
let counter = make_counter()

fn fib(n) {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}

fn down(n) {
    if n == 0 { return 0 }
    return 1 + down(n - 1)
}

let lambda_handler = fn() { return "lambda" }

@get("/ping")
fn ping() -> Json {
    return { ok: true }
}

@get("/hello/:name")
fn hello(name) {
    return { message: "{config.greeting}, {name}!", tags: config.tags }
}

@get("/org/:org/repo/:repo")
fn repo(org, repo, tab) {
    return { org: org, repo: repo, tab: tab }
}

@get("/search")
fn search(q, page, query) {
    return { q: q, page: page, all: query }
}

@post("/items")
fn create(body) {
    return { created: body, keys: len(keys(body)) }
}

@put("/items/:id")
fn update(id, data) {
    return { id: id, data: data }
}

@delete("/items/:id")
fn remove(id) {
    return { deleted: id }
}

@get("/hits")
fn hit() {
    hits = hits + 1
    log.push(hits)
    return { hits: hits, log: log }
}

@get("/counter")
fn count() {
    counter()
    return { n: counter() }
}

@get("/fib/:n")
fn fib_route(n) {
    return { n: int(n), fib: fib(int(n)) }
}

@get("/deep")
fn deep() {
    return { depth: down(2000) }
}

@get("/shapes")
fn shapes() {
    return {
        list: [1, 2.5, "x", null, true],
        ok: Ok(1),
        err: Err("no"),
        nested: { a: [{ b: 1 }] },
        some: Some(3),
        fun: fib
    }
}

@get("/text")
fn text() {
    return "plain string"
}

@get("/nothing")
fn nothing() {
    let x = 1
}

@get("/must")
fn fail() {
    return must Err("boom")
}

@get("/panic")
fn panic_route() {
    bruh("handler gave up")
}

@get("/undefined")
fn undefined_call() {
    return not_defined_anywhere(1)
}

@get("/caught")
fn caught() {
    try {
        must Err("inner")
    } catch e {
        return { caught: true }
    }
}

@get("/lambda")
fn lambda_route() {
    return lambda_handler()
}

@ws("/echo")
fn echo(msg) {
    log.push(msg)
    return "echo {len(log)}: {msg}"
}
"#;

/// `(method, path, JSON body)`.
const REQUESTS: &[(&str, &str, Option<&str>)] = &[
    ("GET", "/ping", None),
    ("GET", "/hello/forge", None),
    ("GET", "/hello/caf%C3%A9", None),
    ("GET", "/org/acme/repo/rocket?tab=issues", None),
    ("GET", "/org/acme/repo/rocket", None),
    ("GET", "/search?q=vm&page=2&extra=1", None),
    ("GET", "/search", None),
    (
        "POST",
        "/items",
        Some(r#"{"name":"widget","price":9.5,"tags":["x"]}"#),
    ),
    ("POST", "/items", Some(r#"{}"#)),
    ("POST", "/items", Some(r#"not json"#)),
    ("POST", "/items", None),
    ("PUT", "/items/42", Some(r#"{"name":"gadget"}"#)),
    ("DELETE", "/items/42", None),
    ("GET", "/hits", None),
    ("GET", "/hits", None),
    ("GET", "/counter", None),
    ("GET", "/counter", None),
    ("GET", "/fib/15", None),
    ("GET", "/deep", None),
    ("GET", "/shapes", None),
    ("GET", "/text", None),
    ("GET", "/nothing", None),
    ("GET", "/must", None),
    ("GET", "/panic", None),
    ("GET", "/undefined", None),
    ("GET", "/caught", None),
    ("GET", "/lambda", None),
    ("GET", "/no/such/route", None),
    ("POST", "/ping", Some("{}")),
    ("DELETE", "/hello/forge", None),
];

#[derive(Debug, PartialEq)]
struct Reply {
    status: u16,
    content_type: Option<String>,
    body: String,
}

fn send(
    client: &reqwest::blocking::Client,
    port: u16,
    request: &(&str, &str, Option<&str>),
) -> Reply {
    let (method, path, body) = *request;
    let url = format!("http://127.0.0.1:{}{}", port, path);
    let mut builder = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        other => panic!("unsupported method {other}"),
    };
    if let Some(body) = body {
        builder = builder
            .header("content-type", "application/json")
            .body(body.to_string());
    }
    let response = builder.send().expect("send");
    Reply {
        status: response.status().as_u16(),
        content_type: response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().expect("ascii content type").to_string()),
        body: response.text().expect("body"),
    }
}

fn replies(engine: Engine) -> Vec<Reply> {
    let port = spawn_server_on(
        PROGRAM,
        engine,
        forge_lang::runtime::recursion::configure_runtime,
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("client");
    REQUESTS
        .iter()
        .map(|request| send(&client, port, request))
        .collect()
}

#[test]
fn http_responses_are_identical_on_both_engines() {
    let vm = replies(Engine::Vm);
    let interp = replies(Engine::Interpreter);
    let mut mismatches = Vec::new();
    for ((request, vm), interp) in REQUESTS.iter().zip(&vm).zip(&interp) {
        if vm != interp {
            mismatches.push(format!(
                "{} {}\n    vm:          {:?}\n    interpreter: {:?}",
                request.0, request.1, vm, interp
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "engines disagree on {} of {} requests:\n  {}",
        mismatches.len(),
        REQUESTS.len(),
        mismatches.join("\n  ")
    );

    // Sanity: the corpus exercises what it claims to.
    let status_of = |path: &str| {
        REQUESTS
            .iter()
            .zip(&vm)
            .find(|((_, p, _), _)| *p == path)
            .map(|(_, reply)| reply.status)
            .expect("request in corpus")
    };
    assert_eq!(status_of("/hello/forge"), 200);
    assert_eq!(status_of("/must"), 500);
    assert_eq!(status_of("/panic"), 500);
    assert_eq!(status_of("/no/such/route"), 404);
    assert_eq!(status_of("/ping"), 200);
    let hits: Vec<_> = REQUESTS
        .iter()
        .zip(&vm)
        .filter(|((_, p, _), _)| *p == "/hits")
        .map(|(_, reply)| reply.body.as_str())
        .collect();
    assert_eq!(
        hits,
        [r#"{"hits":1,"log":[1]}"#, r#"{"hits":1,"log":[1]}"#],
        "a handler's global mutation leaked into the next request"
    );
}

fn ws_transcript(engine: Engine) -> Vec<String> {
    let port = spawn_server_on(
        PROGRAM,
        engine,
        forge_lang::runtime::recursion::configure_runtime,
    );
    let url = format!("ws://127.0.0.1:{}/echo", port);
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let mut transcript = Vec::new();
        // Two connections: state persists within one, never across.
        for messages in [&["hi", "there", "again"][..], &["fresh"][..]] {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("connect websocket");
            for message in messages {
                ws.send(Message::Text((*message).into()))
                    .await
                    .expect("send");
                let reply = tokio::time::timeout(Duration::from_secs(10), ws.next())
                    .await
                    .expect("reply in time")
                    .expect("stream open")
                    .expect("frame");
                transcript.push(reply.into_text().expect("text frame").to_string());
            }
            let _ = ws.close(None).await;
        }
        transcript
    })
}

#[test]
fn websocket_echo_is_identical_on_both_engines() {
    let vm = ws_transcript(Engine::Vm);
    assert_eq!(
        vm,
        [
            "echo 1: hi",
            "echo 2: there",
            "echo 3: again",
            "echo 1: fresh"
        ]
    );
    assert_eq!(vm, ws_transcript(Engine::Interpreter));
}
