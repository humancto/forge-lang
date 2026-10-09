//! End-to-end tests for `forge mcp` over real stdio pipes (newline-delimited
//! JSON-RPC, the MCP stdio transport).
//!
//! Every read has a timeout and the child is killed on drop, so a server
//! regression fails the test instead of hanging CI. Every line the server
//! writes to stdout must be a JSON-RPC message: anything else (e.g. a
//! script's output leaking past the sandbox capture) fails the test.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(30);

struct McpServer {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Result<Value, String>>,
    next_id: i64,
}

impl McpServer {
    fn spawn(args: &[&str], dir: Option<&Path>) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_forge"));
        cmd.arg("mcp")
            .args(args)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        let mut child = cmd.spawn().expect("spawn forge mcp");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("child stdout");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let msg = serde_json::from_str::<Value>(&line)
                    .map_err(|_| line.clone())
                    .and_then(|v| {
                        if v["jsonrpc"] == "2.0" {
                            Ok(v)
                        } else {
                            Err(line.clone())
                        }
                    });
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        McpServer {
            child,
            stdin,
            lines: rx,
            next_id: 1,
        }
    }

    fn write_line(&mut self, text: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(text.as_bytes()).expect("write");
        stdin.write_all(b"\n").expect("write");
        stdin.flush().expect("flush");
    }

    fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write_line(&msg.to_string());
    }

    /// Send a request and return its id without waiting.
    fn send(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.write_line(&msg.to_string());
        id
    }

    /// Next message from the server; panics on timeout or a non-JSON-RPC line.
    fn recv(&self) -> Value {
        match self.lines.recv_timeout(TIMEOUT) {
            Ok(Ok(msg)) => msg,
            Ok(Err(line)) => panic!("non-protocol output on stdout: {line:?}"),
            Err(e) => panic!("no message from the server within {TIMEOUT:?}: {e}"),
        }
    }

    fn response(&self, id: i64) -> Value {
        let msg = self.recv();
        assert_eq!(msg["id"], json!(id), "unexpected message: {msg}");
        msg
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let msg = self.response(id);
        assert!(msg.get("error").is_none(), "{method} failed: {msg}");
        msg["result"].clone()
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": tool, "arguments": arguments}))
    }

    fn initialize(&mut self) -> Value {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "mcp_stdio test", "version": "1"}
            }),
        );
        self.notify("notifications/initialized", json!({}));
        result
    }

    fn close_and_wait(&mut self) -> ExitStatus {
        self.stdin.take();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(Instant::now() < deadline, "server did not exit after EOF");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A path as the body of a Forge string literal (`\` is an escape in Forge,
/// so Windows paths must be doubled).
fn forge_lit(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

fn text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content: {result}"))
        .to_string()
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forge_mcp_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn mcp_full_session_over_stdio() {
    let mut server = McpServer::spawn(&[], None);

    let init = server.initialize();
    assert_eq!(init["protocolVersion"], "2025-11-25");
    assert!(init["capabilities"]["tools"].is_object(), "{init}");
    assert_eq!(init["serverInfo"]["name"], "forge");

    let tools = server.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "run_forge",
            "check_forge",
            "forge_reference",
            "reset_session"
        ]
    );
    assert!(tools["tools"][0]["inputSchema"]["required"]
        .as_array()
        .is_some_and(|r| r.contains(&json!("code"))));

    // Happy path.
    let ok = server.call("run_forge", json!({"code": "say 1 + 2"}));
    assert_eq!(text(&ok), "3\n");
    assert!(ok.get("isError").is_none(), "{ok}");
    assert_eq!(ok["structuredContent"]["ok"], true);

    // Nothing a script prints or reads touches the protocol stream: tasks,
    // io.print, the io.prompt prompt and input() (stdin is /dev/null).
    let quiet = server.call(
        "run_forge",
        json!({"code": "let h = spawn { say \"task\" }\nawait h\nio.print(\"raw\")\nlet a = io.prompt(\"PROMPT> \")\nlet b = input()\nsay \"[\" + a + b + \"]\""}),
    );
    assert_eq!(text(&quiet), "task\nraw[]\n", "{quiet}");

    // Default deny: files, subprocesses and process control.
    for (code, cap) in [
        ("say \"before\"\nfs.read(\"/etc/hostname\")", "fs.read"),
        ("sh(\"echo pwned\")", "run"),
        ("exit(3)", "process"),
    ] {
        let denied = server.call("run_forge", json!({ "code": code }));
        assert_eq!(denied["isError"], true, "{denied}");
        assert_eq!(
            denied["structuredContent"]["error"]["kind"], "permission_denied",
            "{denied}"
        );
        assert!(
            text(&denied).starts_with(&format!("permission denied: {}", cap)),
            "{denied}"
        );
    }

    // A stuck script times out, and the server answers other requests
    // while it runs.
    let started = Instant::now();
    let slow = server.send(
        "tools/call",
        json!({"name": "run_forge", "arguments": {"code": "say \"spinning\"\nwhile true { }", "timeout_secs": 1}}),
    );
    let ping = server.send("ping", json!({}));
    let first = server.recv();
    assert_eq!(
        first["id"],
        json!(ping),
        "ping must not wait for the script"
    );
    let timed_out = server.response(slow)["result"].clone();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(timed_out["isError"], true);
    assert_eq!(timed_out["structuredContent"]["error"]["kind"], "timeout");
    assert_eq!(timed_out["structuredContent"]["stdout"], "spinning\n");

    // Syntax error.
    let syntax = server.call("run_forge", json!({"code": "let = ="}));
    assert_eq!(syntax["isError"], true);
    assert_eq!(syntax["structuredContent"]["error"]["kind"], "syntax");
    assert!(text(&syntax).starts_with("syntax error"), "{syntax}");

    // The language reference.
    let reference = server.call("forge_reference", json!({}));
    let reference = text(&reference);
    assert!(reference.len() > 1000 && reference.starts_with("# Forge"));

    // Diagnostics without running.
    let checked = server.call("check_forge", json!({"code": "say \"ok\"\nlet x = (1 +"}));
    assert_eq!(checked["structuredContent"]["ok"], false, "{checked}");
    let diag = &checked["structuredContent"]["diagnostics"][0];
    assert_eq!(diag["severity"], "error");
    assert_eq!(diag["line"], 2);
    let clean = server.call("check_forge", json!({"code": "say 1"}));
    assert_eq!(text(&clean), "No problems found.");

    // Protocol errors are answered, never fatal.
    server.write_line("{this is not json");
    assert_eq!(server.recv()["error"]["code"], -32700);
    let unknown = server.send("no/such/method", json!({}));
    assert_eq!(server.response(unknown)["error"]["code"], -32601);
    let after = server.call("run_forge", json!({"code": "say \"still alive\""}));
    assert_eq!(text(&after), "still alive\n");

    // EOF on stdin shuts the server down cleanly.
    let status = server.close_and_wait();
    assert!(status.success(), "{status:?}");
}

#[test]
fn mcp_grants_come_from_flags_and_forge_toml() {
    let dir = temp_dir("grants");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("mkdir");
    std::fs::write(data.join("in.txt"), "granted").expect("write");
    std::fs::write(dir.join("secret.txt"), "secret").expect("write");
    std::fs::write(
        dir.join("forge.toml"),
        "[project]\nname = \"t\"\n\n[permissions]\nallow-env = true\nmax-time = 5\n",
    )
    .expect("write forge.toml");

    let read_flag = format!("--allow-read={}", data.display());
    let mut server = McpServer::spawn(&[&read_flag], Some(&dir));
    server.initialize();

    let tools = server.request("tools/list", json!({}));
    let description = tools["tools"][0]["description"].as_str().expect("desc");
    assert!(description.contains("fs.read ("), "{description}");
    assert!(description.contains("env"), "{description}");
    assert!(description.contains("Time limit: 5s"), "{description}");

    let inside = server.call(
        "run_forge",
        json!({"code": format!("say fs.read(\"{}\")", forge_lit(&data.join("in.txt")))}),
    );
    assert_eq!(text(&inside), "granted\n", "{inside}");
    let outside = server.call(
        "run_forge",
        json!({"code": format!("say fs.read(\"{}\")", forge_lit(&dir.join("secret.txt")))}),
    );
    assert_eq!(
        outside["structuredContent"]["error"]["kind"],
        "permission_denied"
    );
    // forge.toml granted env; nothing granted net or run.
    let env = server.call("run_forge", json!({"code": "say env.has(\"PATH\")"}));
    assert_eq!(text(&env), "true\n", "{env}");
    let net = server.call(
        "run_forge",
        json!({"code": "http.get(\"https://example.com\")"}),
    );
    assert!(text(&net).starts_with("permission denied: net"), "{net}");
    // timeout_secs cannot exceed the server's limit (5s from forge.toml).
    let capped = server.call(
        "run_forge",
        json!({"code": "say 1", "timeout_secs": 100000}),
    );
    assert_eq!(text(&capped), "1\n");

    assert!(server.close_and_wait().success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mcp_modern_stateless_requests() {
    let mut server = McpServer::spawn(&["--allow-run"], None);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "t", "version": "1"}
    });
    // No initialize: every request stands alone.
    let discover = server.request("server/discover", json!({ "_meta": meta }));
    assert_eq!(discover["resultType"], "complete");
    assert!(discover["supportedVersions"]
        .as_array()
        .is_some_and(|v| v.contains(&json!("2026-07-28")) && v.contains(&json!("2025-06-18"))));
    let shell = server.request(
        "tools/call",
        json!({"name": "run_forge", "arguments": {"code": "say sh(\"echo granted\")"}, "_meta": meta}),
    );
    assert_eq!(shell["resultType"], "complete");
    assert_eq!(
        shell["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "forge"
    );
    assert!(text(&shell).starts_with("granted"), "{shell}");
    let bad = server.send(
        "tools/list",
        json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "1999-01-01", "io.modelcontextprotocol/clientCapabilities": {}}}),
    );
    let bad = server.response(bad);
    assert_eq!(bad["error"]["code"], -32022);
    assert_eq!(bad["error"]["data"]["requested"], "1999-01-01");
    assert!(server.close_and_wait().success());
}

fn example_tools() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/mcp/weather_tools.fg")
}

fn structured(result: &Value) -> Value {
    result["structuredContent"].clone()
}

#[test]
fn mcp_serve_example_tools() {
    let example = example_tools();
    let mut server = McpServer::spawn(&["serve", example.to_str().expect("utf8 path")], None);
    let init = server.initialize();
    assert!(init["capabilities"]["resources"].is_object(), "{init}");

    // Only the file's tools: no arbitrary code execution unless asked.
    let tools = server.request("tools/list", json!({}));
    let tools = tools["tools"].as_array().expect("tools").clone();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        [
            "convert_temperature",
            "convert_distance",
            "text_stats",
            "weather"
        ]
    );
    let convert = &tools[0];
    assert_eq!(
        convert["inputSchema"]["properties"]["value"],
        json!({"type": "number", "description": "The temperature to convert"})
    );
    assert_eq!(
        convert["inputSchema"]["properties"]["target"]["default"],
        "F"
    );
    assert_eq!(convert["inputSchema"]["required"], json!(["value"]));
    assert_eq!(convert["inputSchema"]["additionalProperties"], false);
    assert_eq!(
        convert["outputSchema"]["properties"]["result"]["type"],
        "number"
    );
    assert_eq!(convert["annotations"]["idempotentHint"], true);
    assert_eq!(convert["annotations"]["readOnlyHint"], true);
    let weather = &tools[3];
    assert_eq!(weather["outputSchema"]["title"], "Report");
    assert!(weather["outputSchema"]["required"]
        .as_array()
        .is_some_and(|r| r.contains(&json!("advice"))));

    let hot = server.call("convert_temperature", json!({"value": 100}));
    assert_eq!(structured(&hot), json!({"result": 212.0}), "{hot}");
    let back = server.call("convert_temperature", json!({"value": 212, "target": "C"}));
    assert_eq!(structured(&back)["result"], 100.0);
    let bad_unit = server.call("convert_temperature", json!({"value": 1, "target": "K"}));
    assert_eq!(bad_unit["isError"], true);
    assert!(
        text(&bad_unit).contains("must be \"C\" or \"F\""),
        "{bad_unit}"
    );

    // Schema validation errors are tool errors the model can act on.
    let invalid = server.call("convert_temperature", json!({"value": "hot", "scale": 1}));
    assert_eq!(invalid["isError"], true);
    let message = text(&invalid);
    assert!(
        message.contains("`value`: expected a number, got a string"),
        "{message}"
    );
    assert!(message.contains("`scale`: unknown argument"), "{message}");
    let missing = server.call("convert_distance", json!({"value": 5}));
    assert!(
        text(&missing).contains("`unit`: missing required argument"),
        "{missing}"
    );

    let lisbon = server.call("weather", json!({"city": "lisbon"}));
    let report = structured(&lisbon);
    assert_eq!(report["city"], "Lisbon", "{lisbon}");
    assert_eq!(report["temperature"], 24.5);
    assert_eq!(report["advice"], "Enjoy the day.");
    assert!(report.get("__type__").is_none(), "{report}");
    let oslo_f = server.call("weather", json!({"city": "Oslo", "unit": "F"}));
    assert_eq!(structured(&oslo_f)["temperature"], 26.6);
    let nowhere = server.call("weather", json!({"city": "Atlantis"}));
    assert_eq!(nowhere["isError"], true);
    assert!(text(&nowhere).contains("known cities: Lisbon"), "{nowhere}");

    let stats = server.call("text_stats", json!({"text": "hello big world\nbye"}));
    assert_eq!(
        structured(&stats),
        json!({"characters": 19, "words": 4, "lines": 2, "longest_word": "hello"})
    );

    let resources = server.request("resources/list", json!({}));
    assert_eq!(resources["resources"][0]["uri"], "weather://cities");
    let cities = server.request("resources/read", json!({"uri": "weather://cities"}));
    let body = cities["contents"][0]["text"].as_str().expect("text");
    assert!(body.contains("Singapore"), "{body}");

    // run_forge is not served by a tool server unless asked.
    let refused = server.send(
        "tools/call",
        json!({"name": "run_forge", "arguments": {"code": "say 1"}}),
    );
    assert_eq!(server.response(refused)["error"]["code"], -32602);
    assert!(server.close_and_wait().success());
}

#[test]
fn mcp_serve_tools_keep_the_sandbox() {
    let dir = temp_dir("tools");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("mkdir");
    std::fs::write(data.join("note.txt"), "inside").expect("write");
    std::fs::write(dir.join("secret.txt"), "secret").expect("write");
    std::fs::write(dir.join("helpers.fg"), "fn shout(s) { return s.upper() }\n").expect("write");
    let tools = dir.join("tools.fg");
    std::fs::write(
        &tools,
        r#"import "helpers.fg"
let mut calls = 0
say "loading tools"

@tool(description: "Read a file")
fn read(path: String) -> String { return fs.read(path) }

@tool(description: "Run a shell command")
fn shell(cmd: String) { return sh(cmd) }

@tool(description: "Spin forever", timeout: 1)
fn spin() { say "spinning"
squad { spawn { while true { } } } }

@tool(description: "Wait on a channel nobody sends to", timeout: 1)
fn stuck() { let ch = channel()
return receive(ch) }

@tool(description: "Count calls")
fn count() -> Int { calls = calls + 1
return calls }

@tool(description: "Shout")
fn loud(s: String) -> String { return shout(s) }

@tool(description: "Read stdin")
fn stdin() { return "[" + input() + "]" }

@tool(description: "Print a lot")
fn spam() { while true { say "spam spam spam spam" } }
"#,
    )
    .expect("write tools");
    let read_flag = format!("--allow-read={}", data.display());
    let mut server = McpServer::spawn(
        &[
            "serve",
            tools.to_str().expect("utf8"),
            "--with-code-tools",
            &read_flag,
        ],
        None,
    );
    server.initialize();
    let listed = server.request("tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        names.starts_with(&[
            "run_forge",
            "check_forge",
            "forge_reference",
            "reset_session",
            "read"
        ]),
        "{names:?}"
    );

    let inside = server.call(
        "read",
        json!({"path": data.join("note.txt").display().to_string()}),
    );
    assert_eq!(structured(&inside)["result"], "inside", "{inside}");
    let outside = server.call(
        "read",
        json!({"path": dir.join("secret.txt").display().to_string()}),
    );
    assert_eq!(outside["isError"], true);
    assert!(
        text(&outside).starts_with("permission denied: fs.read"),
        "{outside}"
    );
    assert_eq!(outside["_meta"]["forge/error"]["kind"], "permission_denied");
    let shell = server.call("shell", json!({"cmd": "echo pwned"}));
    assert!(
        text(&shell).starts_with("permission denied: run"),
        "{shell}"
    );

    // The per-tool limit stops squad tasks and blocked waits (guarantees
    // the VM cannot give yet, which is why tools run on the interpreter).
    for tool in ["spin", "stuck"] {
        let started = Instant::now();
        let r = server.call(tool, json!({}));
        assert_eq!(r["isError"], true, "{r}");
        assert_eq!(r["_meta"]["forge/error"]["kind"], "timeout", "{r}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    // Every call is a fresh fork of the top level.
    for _ in 0..3 {
        assert_eq!(structured(&server.call("count", json!({})))["result"], 1);
    }
    // Imports next to the tool file work; stdin is not readable.
    assert_eq!(
        structured(&server.call("loud", json!({"s": "hi"})))["result"],
        "HI"
    );
    assert_eq!(structured(&server.call("stdin", json!({})))["result"], "[]");
    let spam = server.call("spam", json!({}));
    assert_eq!(
        spam["_meta"]["forge/error"]["kind"], "output_limit",
        "{spam}"
    );

    // The code tools are there too, under the same policy.
    let run = server.call("run_forge", json!({"code": "say 6 * 7"}));
    assert_eq!(text(&run), "42\n");
    assert!(server.close_and_wait().success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mcp_serve_rejects_bad_tool_files() {
    let dir = temp_dir("badtools");
    let file = dir.join("bad.fg");
    std::fs::write(&file, "let x = 1\n@tool\nfn f() {}\n").expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(["mcp", "serve", file.to_str().expect("utf8")])
        .stdin(Stdio::null())
        .output()
        .expect("run forge");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bad.fg:2: @tool on `f` needs a description"),
        "{stderr}"
    );
    assert!(out.stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn run_in(server: &mut McpServer, session: &str, code: &str) -> Value {
    server.call("run_forge", json!({"code": code, "session_id": session}))
}

#[test]
fn mcp_run_forge_sessions() {
    let mut server = McpServer::spawn(&["--max-sessions", "2"], None);
    server.initialize();

    let first = run_in(&mut server, "a", "let x = 41\nfn twice(n) { return n * 2 }");
    assert_eq!(
        structured(&first)["session"],
        json!({"id": "a", "created": true, "lost": false})
    );
    let second = run_in(&mut server, "a", "say x + 1\nsay twice(x)");
    assert_eq!(text(&second), "42\n82\n", "{second}");
    assert_eq!(structured(&second)["session"]["created"], false);

    // Sessions are isolated from each other and from stateless calls.
    let other = run_in(&mut server, "b", "say x");
    assert_eq!(other["isError"], true, "{other}");
    let stateless = server.call("run_forge", json!({"code": "say x"}));
    assert_eq!(stateless["isError"], true);

    // A failing step keeps what it defined before the error.
    let failing = run_in(&mut server, "a", "let y = 1\nlet z = 1 / 0");
    assert_eq!(failing["isError"], true);
    assert_eq!(text(&run_in(&mut server, "a", "say y")), "1\n");

    // A timed-out step stops; the session survives.
    let slow = server.call(
        "run_forge",
        json!({"code": "while true { }", "session_id": "a", "timeout_secs": 0.5}),
    );
    assert_eq!(structured(&slow)["error"]["kind"], "timeout");
    assert_eq!(structured(&slow)["session"]["lost"], false);
    assert_eq!(text(&run_in(&mut server, "a", "say x")), "41\n");

    // The number of sessions is bounded.
    let full = run_in(&mut server, "c", "say 1");
    assert_eq!(structured(&full)["error"]["kind"], "session", "{full}");
    assert!(text(&full).contains("at most 2 sessions"), "{full}");

    // Reset drops the state and frees the slot.
    let reset = server.call("reset_session", json!({"session_id": "a"}));
    assert_eq!(structured(&reset), json!({"existed": true}));
    let again = server.call("reset_session", json!({"session_id": "a"}));
    assert_eq!(structured(&again), json!({"existed": false}));
    let fresh = run_in(&mut server, "a", "say x");
    assert_eq!(fresh["isError"], true);
    assert_eq!(structured(&fresh)["session"]["created"], true);

    // Resetting a session stops the call running in it; that call still
    // gets a response (it was not cancelled by the client).
    let running = server.send(
        "tools/call",
        json!({"name": "run_forge", "arguments": {"code": "say \"start\"\nwhile true { }", "session_id": "a"}}),
    );
    std::thread::sleep(Duration::from_millis(300));
    let busy = run_in(&mut server, "a", "say 1");
    assert_eq!(structured(&busy)["error"]["kind"], "session", "{busy}");
    let reset = server.send(
        "tools/call",
        json!({"name": "reset_session", "arguments": {"session_id": "a"}}),
    );
    let mut replies = vec![server.recv(), server.recv()];
    replies.sort_by_key(|m| m["id"].as_i64());
    assert_eq!(replies[0]["id"], json!(running));
    assert_eq!(replies[1]["id"], json!(reset));
    let stopped = replies[0]["result"].clone();
    assert_eq!(
        structured(&stopped)["error"]["kind"],
        "cancelled",
        "{stopped}"
    );
    assert_eq!(structured(&stopped)["session"]["lost"], true);
    assert!(
        text(&stopped).contains("reset while this call ran"),
        "{stopped}"
    );
    assert_eq!(structured(&replies[1]["result"]), json!({"existed": true}));

    let bad = run_in(&mut server, "not valid!", "say 1");
    assert_eq!(bad["isError"], true);
    assert!(server.close_and_wait().success());
}
