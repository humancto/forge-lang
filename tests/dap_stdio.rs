//! End-to-end tests for `forge dap` over real stdio pipes.
//!
//! These cover the parts a real client (VS Code) depends on: framing,
//! breakpoints set during the configuration phase taking effect, and
//! `stopped` / `output` / `terminated` events arriving *unprompted* (the
//! client sends nothing while the program runs).

#[path = "support/stdio_rpc.rs"]
mod stdio_rpc;

use serde_json::{json, Value};
use stdio_rpc::StdioServer;

struct Dap {
    server: StdioServer,
    seq: i64,
}

impl Dap {
    fn start() -> Self {
        Dap {
            server: StdioServer::spawn("dap"),
            seq: 0,
        }
    }

    /// Send a request and return its (successful) response.
    fn request(&mut self, command: &str, arguments: Value) -> Value {
        self.seq += 1;
        let seq = self.seq;
        self.server.send(json!({
            "seq": seq, "type": "request", "command": command, "arguments": arguments
        }));
        let resp = self
            .server
            .recv_until(&format!("response to {}", command), |m| {
                m["type"] == "response" && m["request_seq"] == json!(seq)
            });
        assert_eq!(resp["success"], true, "{} failed: {}", command, resp);
        assert_eq!(resp["command"], command);
        resp
    }

    fn event(&self, name: &str) -> Value {
        self.server.recv_until(&format!("{} event", name), |m| {
            m["type"] == "event" && m["event"] == name
        })
    }
}

fn write_program(name: &str, source: &str) -> String {
    let dir = std::env::temp_dir().join(format!("forge-dap-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path.to_string_lossy().into_owned()
}

fn initialize(dap: &mut Dap) {
    let resp = dap.request(
        "initialize",
        json!({"adapterID": "forge", "linesStartAt1": true}),
    );
    assert_eq!(resp["body"]["supportsConfigurationDoneRequest"], true);
    dap.event("initialized");
}

#[test]
fn dap_breakpoint_session_over_stdio() {
    let program = write_program(
        "bp.fg",
        "let x = 41\nlet y = x + 1\nprintln(y)\nprintln(\"done\")\n",
    );
    let mut dap = Dap::start();
    initialize(&mut dap);

    // VS Code order: launch, then breakpoints, then configurationDone.
    dap.request("launch", json!({"program": program, "stopOnEntry": false}));
    let bps = dap.request(
        "setBreakpoints",
        json!({"source": {"path": program}, "breakpoints": [{"line": 3}]}),
    );
    assert_eq!(bps["body"]["breakpoints"][0]["verified"], true);
    dap.request("configurationDone", json!({}));

    // The program must start and hit the breakpoint with no further requests.
    let stopped = dap.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    let threads = dap.request("threads", Value::Null);
    assert_eq!(threads["body"]["threads"][0]["id"], 1);

    let trace = dap.request("stackTrace", json!({"threadId": 1}));
    assert_eq!(trace["body"]["stackFrames"][0]["line"], 3);

    dap.request("scopes", json!({"frameId": 0}));
    let vars = dap.request("variables", json!({"variablesReference": 1}));
    let vars = vars["body"]["variables"].as_array().unwrap().clone();
    let y = vars.iter().find(|v| v["name"] == "y").expect("variable y");
    assert_eq!(y["value"], "42");

    dap.request("continue", json!({"threadId": 1}));

    // Output and termination are pushed without any client request.
    let mut output = String::new();
    loop {
        let msg = dap.server.recv_until("output/terminated", |m| {
            m["type"] == "event" && (m["event"] == "output" || m["event"] == "terminated")
        });
        if msg["event"] == "terminated" {
            break;
        }
        output.push_str(msg["body"]["output"].as_str().unwrap_or(""));
    }
    assert!(output.contains("42"), "output: {:?}", output);
    assert!(output.contains("done"), "output: {:?}", output);

    dap.request("disconnect", json!({}));
    assert!(dap.server.wait_exit().success());
}

#[test]
fn dap_breakpoints_match_by_file_path() {
    let program = write_program("paths.fg", "let a = 1\nlet b = 2\nlet c = 3\n");
    let other = write_program("other.fg", "say 1\nsay 2\n");
    let mut dap = Dap::start();
    initialize(&mut dap);
    dap.request("launch", json!({"program": program, "stopOnEntry": false}));
    // Line 2 of a different file must not stop this program.
    dap.request(
        "setBreakpoints",
        json!({"source": {"path": other}, "breakpoints": [{"line": 2}]}),
    );
    // The same program spelled differently (`dir/./paths.fg`) still matches.
    let path = std::path::Path::new(&program);
    let respelled = path
        .parent()
        .expect("temp file has a parent")
        .join(".")
        .join(path.file_name().expect("temp file has a name"))
        .to_string_lossy()
        .into_owned();
    dap.request(
        "setBreakpoints",
        json!({"source": {"path": respelled}, "breakpoints": [{"line": 3}]}),
    );
    dap.request("configurationDone", json!({}));

    let stopped = dap.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");
    let trace = dap.request("stackTrace", json!({"threadId": 1}));
    assert_eq!(trace["body"]["stackFrames"][0]["line"], 3);

    dap.request("continue", json!({"threadId": 1}));
    dap.event("terminated");
    dap.request("disconnect", json!({}));
    assert!(dap.server.wait_exit().success());
}

#[test]
fn dap_stop_on_entry_and_step() {
    let program = write_program("step.fg", "let a = 1\nlet b = 2\nlet c = 3\n");
    let mut dap = Dap::start();
    initialize(&mut dap);
    dap.request("launch", json!({"program": program, "stopOnEntry": true}));
    dap.request("configurationDone", json!({}));

    let stopped = dap.event("stopped");
    assert_eq!(stopped["body"]["reason"], "entry");

    dap.request("next", json!({"threadId": 1}));
    let stopped = dap.event("stopped");
    assert_eq!(stopped["body"]["reason"], "step");
    let trace = dap.request("stackTrace", json!({"threadId": 1}));
    assert_eq!(trace["body"]["stackFrames"][0]["line"], 2);

    dap.request("continue", json!({"threadId": 1}));
    let exited = dap.event("exited");
    assert_eq!(exited["body"]["exitCode"], 0);
    dap.event("terminated");
}

#[test]
fn dap_launch_failure_reports_and_terminates() {
    let mut dap = Dap::start();
    initialize(&mut dap);
    dap.request("configurationDone", json!({}));
    // Launch after configurationDone starts immediately.
    dap.request(
        "launch",
        json!({"program": "/nonexistent/forge-dap-missing.fg"}),
    );
    let out = dap.event("output");
    assert!(out["body"]["output"]
        .as_str()
        .unwrap()
        .contains("Launch failed"));
    dap.event("terminated");
}

#[test]
fn dap_exits_on_stdin_eof() {
    let mut dap = Dap::start();
    initialize(&mut dap);
    dap.server.close_stdin();
    dap.server.wait_exit();
}
