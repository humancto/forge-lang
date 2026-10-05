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
    assert_eq!(names, ["run_forge", "check_forge", "forge_reference"]);
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
