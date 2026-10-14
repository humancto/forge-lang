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
        list: [1, 2.5, 2.0, "x", null, true],
        text: json.parse("\"tab\\tfeed\\f\""),
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

// A handler using top-level bindings declared below it.
@get("/late")
fn late_route() {
    late_hits = late_hits + 1
    return { late: late_value, hits: late_hits }
}

let late_value = "declared after its handler"
let mut late_hits = 0
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
    ("GET", "/late", None),
    ("GET", "/late", None),
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
    // JSON keeps a Float a Float (`2.0`) and uses the short escapes, like
    // `json.compact`.
    let shapes = REQUESTS
        .iter()
        .zip(&vm)
        .find(|((_, p, _), _)| *p == "/shapes")
        .map(|(_, reply)| reply.body.as_str())
        .expect("request in corpus");
    assert!(
        shapes.contains(r#"[1,2.5,2.0,"x",null,true]"#) && shapes.contains(r#""tab\tfeed\f""#),
        "{shapes}"
    );
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
    let late: Vec<_> = REQUESTS
        .iter()
        .zip(&vm)
        .filter(|((_, p, _), _)| *p == "/late")
        .map(|(_, reply)| reply.body.as_str())
        .collect();
    let expected = r#"{"hits":1,"late":"declared after its handler"}"#;
    assert_eq!(
        late,
        [expected, expected],
        "a binding declared after its handler is unreadable or leaked across requests"
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

// ── Handler-controlled responses and request bodies ─────────────────────────

const RESPONSES: &str = r#"
@server(port: __PORT__)

@get("/ping")
fn ping() {
    return { ok: true }
}

@post("/items")
fn create(body, req) {
    if typeof(body) != "Object" {
        return http.response(400, { error: "expected an object", got: body })
    }
    return http.response(201, { item: body, type: get(req.headers, "content-type") }, { Location: "/items/1" })
}

@post("/echo")
fn echo(body) {
    return { body: body, type: typeof(body) }
}

@put("/items/:id")
fn replace(id, req) {
    return { id: id, method: req.method, path: req.path, params: req.params, body: req.body }
}

@patch("/items/:id")
fn patch_item(id, body) {
    return http.response(200, { id: id, patch: body })
}

@delete("/items/:id")
fn remove(id) {
    if id == "404" {
        return http.response(404, { error: "no item {id}" })
    }
    return http.response(204)
}

@get("/text")
fn text() {
    return http.response(200, "hello there", { "Cache-Control": "no-store" })
}

@get("/html")
fn html() {
    return http.response(200, "<p>hi</p>", { "Content-Type": "text/html; charset=utf-8" })
}

@get("/cookies")
fn cookies() {
    return http.response(200, [1, 2], { "Set-Cookie": ["a=1", "b=2"], "X-Count": 2 })
}

@get("/redirect")
fn redirect() {
    return http.response(302, null, { Location: "/text" })
}

@get("/whoami")
fn whoami(request, headers) {
    return { method: request.method, auth: get(headers, "authorization"), custom: get(headers, "x-custom"), query: request.query, body: request.body }
}

@get("/injection")
fn injection() {
    return http.response(200, "x", { "X-Evil": "a\r\nSet-Cookie: pwned=1" })
}

@get("/bad-status")
fn bad_status() {
    return http.response(999, "x")
}

@get("/no-body-204")
fn no_body_204() {
    return http.response(204, { x: 1 })
}

@get("/hand-built")
fn hand_built() {
    let mut r = http.response(201, "made")
    r.status = 202
    return r
}

@get("/hand-built-bad")
fn hand_built_bad() {
    let mut r = http.response(201, "made")
    r.headers = { "Content-Length": "1" }
    return r
}

@get("/plain-object")
fn plain_object() {
    return { status: 404, body: "not a response" }
}

@get("/raises")
fn raises() {
    return must Err("boom")
}

@post("/webhook")
fn webhook(raw_body, req, body) {
    return { raw: raw_body, same: req.raw_body == raw_body, sig: crypto.sha256(raw_body), parsed: body }
}
"#;

/// `(method, path, content type, body, extra headers)`.
type Request2 = (
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static [u8]>,
    &'static [(&'static str, &'static str)],
);

const REQUESTS2: &[Request2] = &[
    (
        "POST",
        "/items",
        Some("application/json"),
        Some(br#"{"a":1}"#),
        &[],
    ),
    (
        "POST",
        "/items",
        Some("application/json; charset=utf-8"),
        Some(br#"{"a":[1,2]}"#),
        &[],
    ),
    (
        "POST",
        "/items",
        Some("application/vnd.api+json"),
        Some(br#"{"v":true}"#),
        &[],
    ),
    (
        "POST",
        "/items",
        Some("application/json"),
        Some(b"{bad"),
        &[],
    ),
    ("POST", "/items", Some("application/json"), Some(b""), &[]),
    ("POST", "/items", None, None, &[]),
    (
        "POST",
        "/items",
        Some("application/x-www-form-urlencoded"),
        Some(b"a=1&b=x+y&a=2"),
        &[],
    ),
    (
        "POST",
        "/items",
        Some("text/plain"),
        Some(b"just text"),
        &[],
    ),
    ("POST", "/echo", None, Some(b"no content type"), &[]),
    ("POST", "/echo", Some("text/csv"), Some(b"a,b\n1,2"), &[]),
    (
        "POST",
        "/echo",
        Some("application/octet-stream"),
        Some(&[0xff, 0xfe, 0x00]),
        &[],
    ),
    (
        "POST",
        "/echo",
        Some("application/json"),
        Some(b"null"),
        &[],
    ),
    (
        "PUT",
        "/items/7",
        Some("application/json"),
        Some(br#"{"name":"x"}"#),
        &[],
    ),
    ("PUT", "/items/7", None, None, &[]),
    (
        "PATCH",
        "/items/7",
        Some("application/json"),
        Some(br#"{"n":2}"#),
        &[],
    ),
    ("DELETE", "/items/7", None, None, &[]),
    ("DELETE", "/items/404", None, None, &[]),
    ("GET", "/text", None, None, &[]),
    ("GET", "/html", None, None, &[]),
    ("GET", "/cookies", None, None, &[]),
    ("GET", "/redirect", None, None, &[]),
    (
        "GET",
        "/whoami?x=1&y=two&x=3",
        None,
        None,
        &[("Authorization", "Bearer t0k"), ("X-Custom", "yes")],
    ),
    ("GET", "/whoami", None, None, &[]),
    ("GET", "/injection", None, None, &[]),
    ("GET", "/bad-status", None, None, &[]),
    ("GET", "/no-body-204", None, None, &[]),
    ("GET", "/hand-built", None, None, &[]),
    ("GET", "/hand-built-bad", None, None, &[]),
    ("GET", "/plain-object", None, None, &[]),
    ("GET", "/raises", None, None, &[]),
    (
        "POST",
        "/webhook",
        Some("application/json"),
        Some(b"{ \"b\": [2],\n  \"a\":1 }\n"),
        &[],
    ),
    ("POST", "/webhook", None, None, &[]),
    (
        "POST",
        "/webhook",
        Some("application/x-www-form-urlencoded"),
        Some(b"a=x+y&c=%ff"),
        &[],
    ),
    (
        "POST",
        "/webhook",
        Some("application/x-www-form-urlencoded"),
        Some(b"a=\xff"),
        &[],
    ),
];

/// Response headers the corpus compares (the rest — date, request id —
/// differ per request).
const COMPARED_HEADERS: &[&str] = &[
    "content-type",
    "location",
    "cache-control",
    "set-cookie",
    "x-count",
    "x-evil",
];

#[derive(Debug, PartialEq)]
struct Reply2 {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

fn send2(client: &reqwest::blocking::Client, port: u16, request: &Request2) -> Reply2 {
    let (method, path, content_type, body, headers) = *request;
    let url = format!("http://127.0.0.1:{}{}", port, path);
    let method = reqwest::Method::from_bytes(method.as_bytes()).expect("method");
    let mut builder = client.request(method, &url);
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    if let Some(body) = body {
        builder = builder.body(body.to_vec());
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = builder.send().expect("send");
    let mut compared = Vec::new();
    for name in COMPARED_HEADERS {
        for value in response.headers().get_all(*name) {
            compared.push((
                name.to_string(),
                value.to_str().expect("ascii header").to_string(),
            ));
        }
    }
    Reply2 {
        status: response.status().as_u16(),
        headers: compared,
        body: response.text().expect("body"),
    }
}

fn replies2(engine: Engine) -> Vec<Reply2> {
    let port = spawn_server_on(
        RESPONSES,
        engine,
        forge_lang::runtime::recursion::configure_runtime,
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    REQUESTS2
        .iter()
        .map(|request| send2(&client, port, request))
        .collect()
}

#[test]
fn handler_responses_and_request_bodies_are_identical_on_both_engines() {
    let vm = replies2(Engine::Vm);
    let interp = replies2(Engine::Interpreter);
    let mut mismatches = Vec::new();
    for ((request, vm), interp) in REQUESTS2.iter().zip(&vm).zip(&interp) {
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
        REQUESTS2.len(),
        mismatches.join("\n  ")
    );

    // What the responses are, not just that the engines agree.
    let reply = |i: usize| &vm[i];
    let header = |i: usize, name: &str| -> Vec<&str> {
        reply(i)
            .headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    };
    let json = |i: usize| -> serde_json::Value {
        serde_json::from_str(&reply(i).body).expect("JSON body")
    };
    let error = |i: usize| -> String { json(i)["error"].as_str().expect("error").to_string() };
    use serde_json::json as j;

    // 201 + Location, JSON body that saw the request's content type.
    assert_eq!(reply(0).status, 201, "{:?}", reply(0));
    assert_eq!(header(0, "location"), ["/items/1"]);
    assert_eq!(header(0, "content-type"), ["application/json"]);
    assert_eq!(json(0), j!({"item": {"a": 1}, "type": "application/json"}));
    assert_eq!(reply(2).status, 201);
    // Invalid JSON: 400 with the server's JSON error, before the handler.
    assert_eq!(reply(3).status, 400);
    assert!(error(3).starts_with("invalid JSON body: "), "{}", error(3));
    // No body: `body` is `{}`.
    assert_eq!(reply(4).status, 201);
    assert_eq!(reply(5).status, 201);
    assert_eq!(json(5)["item"], j!({}));
    // A form is an object of strings (the last of a repeated name wins).
    assert_eq!(json(6)["item"], j!({"a": "2", "b": "x y"}));
    // Text: the handler answers 400 itself.
    assert_eq!(reply(7).status, 400);
    assert_eq!(
        json(7),
        j!({"error": "expected an object", "got": "just text"})
    );
    assert_eq!(json(8), j!({"body": "no content type", "type": "String"}));
    assert_eq!(json(9)["body"], "a,b\n1,2");
    // A body that is not text: 415.
    assert_eq!(reply(10).status, 415);
    assert_eq!(json(11), j!({"body": null, "type": "Null"}));
    assert_eq!(
        json(12),
        j!({"id": "7", "method": "PUT", "path": "/items/7", "params": {"id": "7"}, "body": {"name": "x"}})
    );
    assert_eq!(json(13)["body"], serde_json::Value::Null);
    assert_eq!(json(14), j!({"id": "7", "patch": {"n": 2}}));
    assert_eq!((reply(15).status, reply(15).body.as_str()), (204, ""));
    assert!(header(15, "content-type").is_empty());
    assert_eq!(reply(16).status, 404);
    assert_eq!(json(16), j!({"error": "no item 404"}));
    assert_eq!(header(17, "content-type"), ["text/plain; charset=utf-8"]);
    assert_eq!(header(17, "cache-control"), ["no-store"]);
    assert_eq!(reply(17).body, "hello there");
    assert_eq!(header(18, "content-type"), ["text/html; charset=utf-8"]);
    assert_eq!(reply(18).body, "<p>hi</p>");
    assert_eq!(header(19, "set-cookie"), ["a=1", "b=2"]);
    assert_eq!(header(19, "x-count"), ["2"]);
    assert_eq!(reply(19).body, "[1,2]");
    assert_eq!(reply(20).status, 302);
    assert_eq!(header(20, "location"), ["/text"]);
    assert_eq!(
        json(21),
        j!({"method": "GET", "auth": "Bearer t0k", "custom": "yes", "query": {"x": "3", "y": "two"}, "body": null})
    );
    assert_eq!(json(22)["auth"], serde_json::Value::Null);
    // Header injection is refused: 500, and nothing injected.
    assert_eq!(reply(23).status, 500);
    assert!(header(23, "x-evil").is_empty() && header(23, "set-cookie").is_empty());
    assert!(error(23).contains("line breaks"), "{}", error(23));
    assert_eq!(reply(24).status, 500);
    assert!(error(24).contains("status must be an Int from 200 to 599"));
    assert_eq!(reply(25).status, 500);
    assert!(error(25).contains("cannot have a body"));
    // A response value changed by hand is validated when it is sent.
    assert_eq!((reply(26).status, reply(26).body.as_str()), (202, "made"));
    assert_eq!(reply(27).status, 500);
    assert!(
        error(27).starts_with("invalid http.response: "),
        "{}",
        error(27)
    );
    // An ordinary object shaped like a response is still a 200 body.
    assert_eq!(reply(28).status, 200);
    assert_eq!(json(28), j!({"status": 404, "body": "not a response"}));
    assert_eq!(reply(29).status, 500);
    // `raw_body` / `req.raw_body`: the exact text, next to the decoded body.
    let raw = "{ \"b\": [2],\n  \"a\":1 }\n";
    assert_eq!(
        json(30),
        j!({"raw": raw, "same": true, "sig": sha256_hex(raw), "parsed": {"b": [2], "a": 1}})
    );
    assert_eq!(
        json(31),
        j!({"raw": "", "same": true, "sig": sha256_hex(""), "parsed": {}})
    );
    assert_eq!(json(32)["raw"], "a=x+y&c=%ff");
    assert_eq!(json(32)["parsed"]["a"], "x y");
    // A body that is not UTF-8 is refused whatever its content type.
    assert_eq!(reply(33).status, 415);
}

fn sha256_hex(text: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}
