//! Per-request isolation contract of the VM server template
//! (`vm::serve::VmTemplate`). The VM counterpart of the interpreter's
//! `fork_for_serving_*` tests.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Value as JsonValue};

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::runtime::metadata::top_level_fn_params;
use crate::runtime::server::{HandlerRequest, ServeEngine};
use crate::vm::compiler;
use crate::vm::machine::VM;
use crate::vm::serve::VmTemplate;

fn build_template(source: &str) -> Result<VmTemplate, String> {
    let tokens = Lexer::new(source).tokenize().expect("lex");
    let program = Parser::new(tokens).parse_program().expect("parse");
    let chunk = compiler::compile(&program).expect("compile");
    let mut vm = VM::new();
    vm.defer_host_runtime();
    vm.execute(&chunk).expect("top level runs");
    VmTemplate::new(&vm, top_level_fn_params(&program))
}

fn template(source: &str) -> VmTemplate {
    build_template(source).expect("template")
}

fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn get(template: &VmTemplate, handler: &str) -> (StatusCode, JsonValue) {
    request(template, handler, HandlerRequest::default())
}

fn request(
    template: &VmTemplate,
    handler: &str,
    request: HandlerRequest,
) -> (StatusCode, JsonValue) {
    template.fork_request(flag()).call_http(handler, &request)
}

#[test]
fn handler_mutations_of_globals_do_not_leak_across_requests() {
    let t = template(
        "let mut hits = 0\n\
         let mut seen = []\n\
         fn hit() {\n\
           hits = hits + 1\n\
           seen.push(hits)\n\
           return { hits: hits, seen: seen }\n\
         }\n",
    );
    for _ in 0..3 {
        assert_eq!(
            get(&t, "hit"),
            (StatusCode::OK, json!({"hits": 1, "seen": [1]}))
        );
    }
}

#[test]
fn captured_closure_state_is_isolated_per_request() {
    let t = template(
        "fn make_counter() {\n\
           let mut n = 0\n\
           return fn() { n = n + 1\n return n }\n\
         }\n\
         let counter = make_counter()\n\
         fn twice() { counter()\n return counter() }\n",
    );
    // Within a request the closure keeps its state (2), but every request
    // starts from the template's cell (0), not the previous request's.
    for _ in 0..3 {
        assert_eq!(get(&t, "twice"), (StatusCode::OK, json!(2)));
    }
}

#[test]
fn closures_sharing_a_cell_still_share_it_within_a_request() {
    let t = template(
        "fn make() {\n\
           let mut n = 0\n\
           return { inc: fn() { n = n + 1 }, get: fn() { return n } }\n\
         }\n\
         let c = make()\n\
         fn run() { c.inc()\n c.inc()\n return c.get() }\n",
    );
    assert_eq!(get(&t, "run"), (StatusCode::OK, json!(2)));
    assert_eq!(get(&t, "run"), (StatusCode::OK, json!(2)));
}

#[test]
fn recursive_closure_through_its_own_upvalue_survives_the_fork() {
    let t = template(
        "fn make() {\n\
           let mut fact = null\n\
           fact = fn(n) { if n <= 1 { return 1 }\n return n * fact(n - 1) }\n\
           return fact\n\
         }\n\
         let fact = make()\n\
         fn run() { return fact(10) }\n",
    );
    assert_eq!(get(&t, "run"), (StatusCode::OK, json!(3628800)));
}

#[test]
fn concurrent_forks_are_independent() {
    let t = Arc::new(template(
        "let mut total = 0\n\
         fn add(n) { total = total + int(n)\n return total }\n",
    ));
    let handles: Vec<_> = (0..16)
        .map(|i| {
            let t = Arc::clone(&t);
            std::thread::spawn(move || {
                let mut req = HandlerRequest::default();
                req.query_params.insert("n".to_string(), i.to_string());
                request(&t, "add", req)
            })
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        assert_eq!(h.join().expect("thread"), (StatusCode::OK, json!(i)));
    }
}

#[test]
fn handler_arguments_bind_like_the_interpreter() {
    let t = template(
        "fn show(id, body, query, page, missing) {\n\
           return { id: id, body: body, query: query, page: page, missing: missing }\n\
         }\n",
    );
    let mut req = HandlerRequest::default();
    req.path_params.insert("id".to_string(), "7".to_string());
    req.query_params.insert("page".to_string(), "2".to_string());
    req.body = Some(json!({"name": "forge", "tags": [1, 2.5, null, true]}));
    let (status, body) = request(&t, "show", req);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "id": "7",
            "body": {"name": "forge", "tags": [1, 2.5, null, true]},
            "query": {"page": "2"},
            "page": "2",
            "missing": null,
        })
    );
}

#[test]
fn errors_and_missing_handlers_are_500s() {
    let t = template("fn boom() { bruh(\"kaboom\") }\n");
    let (status, body) = get(&t, "boom");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        body["error"].as_str().is_some_and(|e| e.contains("kaboom")),
        "{body}"
    );
    assert_eq!(
        get(&t, "nope"),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error": "handler 'nope' not found"})
        )
    );
}

#[test]
fn cancelled_flag_stops_a_running_handler() {
    let t = template("fn spin() { let mut i = 0\n while true { i = i + 1 }\n return i }\n");
    let cancelled = flag();
    cancelled.store(true, Ordering::Release);
    let (status, body) = t
        .fork_request(cancelled)
        .call_http("spin", &HandlerRequest::default());
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"error": "task cancelled"}));
}

#[test]
fn json_encoding_matches_the_interpreter() {
    // Every shape goes through both `value_to_json` (VM) and
    // `forge_to_json` (interpreter) for the same source.
    let source = "fn v() {\n\
           return {\n\
             i: 1, f: 2.5, b: true, n: null, s: \"x\", big: 9007199254740993,\n\
             arr: [1, [2]], ok: Ok(1), err: Err(\"e\"),\n\
             some: Some(1), none: None, tup: (1, 2), set: set([1]),\n\
             fun: fn() { return 1 }, builtin: len, frozen: freeze({a: 1})\n\
           }\n\
         }\n";
    let (_, vm_json) = get(&template(source), "v");

    let tokens = Lexer::new(source).tokenize().expect("lex");
    let program = Parser::new(tokens).parse_program().expect("parse");
    let mut interp = crate::interpreter::Interpreter::new();
    interp.run(&program).expect("interpreter run");
    let (_, interp_json) = crate::runtime::server::InterpreterTemplate::new(interp)
        .fork_request(flag())
        .call_http("v", &HandlerRequest::default());
    assert_eq!(vm_json, interp_json);
}

#[test]
fn websocket_connection_keeps_state_across_messages() {
    let t = template(
        "let mut log = []\n\
         fn echo(msg) { log.push(msg)\n return \"{len(log)}:{msg}\" }\n",
    );
    let mut conn = t.fork_connection(flag());
    assert_eq!(conn.call_ws("echo", "a".to_string()), "1:a");
    assert_eq!(conn.call_ws("echo", "b".to_string()), "2:b");
    // A new connection starts from the template.
    let mut other = t.fork_connection(flag());
    assert_eq!(other.call_ws("echo", "c".to_string()), "1:c");
    assert_eq!(
        conn.call_ws("missing", "x".to_string()).as_str(),
        "handler not found"
    );
}

#[test]
fn top_level_stream_is_rejected_before_serving() {
    let err = build_template("let s = [1, 2, 3].stream()\nfn h() { return s.count() }\n")
        .err()
        .expect("a top-level stream must be rejected");
    assert!(err.contains("stream"), "{err}");
}

#[test]
fn deferred_schedule_is_not_started_by_top_level() {
    // With host runtime deferred, `schedule` records the block and starts
    // nothing until `launch_deferred_host_tasks`; the template must still
    // build (the queued closure is not part of the template).
    let t = template("schedule every 1 hours { say \"tick\" }\nfn h() { return 1 }\n");
    assert_eq!(get(&t, "h"), (StatusCode::OK, json!(1)));
}

#[test]
fn fork_does_not_reregister_builtins() {
    // The fork copies the template's builtins and stdlib modules; it must
    // expose exactly the same globals as the template VM.
    let source = "fn h() { return [math.sqrt(16), len(\"abc\"), json.stringify({a: 1})] }\n";
    let tokens = Lexer::new(source).tokenize().expect("lex");
    let program = Parser::new(tokens).parse_program().expect("parse");
    let chunk = compiler::compile(&program).expect("compile");
    let mut vm = VM::new();
    vm.execute(&chunk).expect("run");
    let t = VmTemplate::new(&vm, HashMap::new()).expect("template");
    let fork = t.fork(flag());
    let mut template_names: Vec<_> = vm.globals.keys().cloned().collect();
    let mut fork_names: Vec<_> = fork.globals.keys().cloned().collect();
    template_names.sort();
    fork_names.sort();
    assert_eq!(template_names, fork_names);
    assert_eq!(
        get(&template(source), "h"),
        (StatusCode::OK, json!([4.0, 3, "{\"a\": 1}"]))
    );
}
