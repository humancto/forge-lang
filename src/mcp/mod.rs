//! `forge mcp`: a Model Context Protocol server that lets AI agents run Forge
//! in a sandbox ("code mode": instead of many tool calls, the agent writes a
//! short script that runs with exactly the capabilities the operator granted).
//!
//! Transport: JSON-RPC 2.0, one message per line, over stdio (or any
//! `BufRead`/`Write` pair via [`serve`]).
//!
//! Protocol: dual-era.
//! * **Modern** (`2026-07-28`): stateless; every request carries
//!   `_meta["io.modelcontextprotocol/protocolVersion"]` and client
//!   capabilities; `server/discover` advertises versions; results carry
//!   `resultType` and `_meta["io.modelcontextprotocol/serverInfo"]`.
//! * **Legacy** (`2025-11-25` back to `2024-11-05`): `initialize` handshake,
//!   then plain requests. `ping` is answered in both eras.
//!
//! Tools: `run_forge` (sandboxed execution, optionally in a persistent
//! session, see [`session`]), `reset_session`, `check_forge` (parse +
//! typecheck diagnostics) and `forge_reference` (the language guide,
//! `llms.txt`). `forge mcp serve tools.fg` adds tools and resources written
//! in Forge (`@tool` / `@resource` functions, see [`tools`]).
//!
//! Engine: every piece of agent-supplied or tool code runs on the
//! tree-walking interpreter inside [`Sandbox`], never on the VM. The VM does
//! not yet provide the sandbox's guarantees: `say` writes straight to the
//! process's stdout (no capture or output budget), a deadline cannot stop
//! `squad` tasks or a blocked `receive` (SECURITY_AUDIT.md SEC-02, VM
//! part), and converting a very deeply nested value can overflow the native
//! stack (SEC-16), which would take the whole server down.
//!
//! Robustness: every `tools/call` that runs code gets its own thread, so a
//! stuck script never blocks the protocol loop; scripts are bounded by a
//! wall-clock limit and an output cap; malformed input gets a JSON-RPC error
//! and never ends the server; `notifications/cancelled` stops a running
//! script. [`serve_stdio`] moves the protocol onto private file descriptors
//! and points the process's stdin at `/dev/null` and stdout at stderr, so
//! nothing a script (or the runtime) prints or reads can corrupt the stream.

pub mod session;
pub mod tools;

pub use tools::ToolSet;

use crate::permissions::Capabilities;
use crate::sandbox::{parse_source, truncate_utf8, CancelHandle, Output, Sandbox, SandboxError};
use serde_json::{json, Map, Value};
use session::{Checkout, Sessions};
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Protocol revisions served statelessly (per-request `_meta`).
pub const MODERN_VERSIONS: &[&str] = &["2026-07-28"];
/// Protocol revisions served after an `initialize` handshake, newest first.
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Default (and maximum) wall-clock time for one `run_forge` call.
pub const DEFAULT_MAX_TIME: Duration = Duration::from_secs(30);
/// Default cap on the script output returned to the agent.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// Default number of tool calls that may run at once.
pub const DEFAULT_MAX_CONCURRENT_CALLS: usize = 8;
/// Default number of live `run_forge` sessions.
pub const DEFAULT_MAX_SESSIONS: usize = 16;
/// Default idle time after which a session is dropped.
pub const DEFAULT_SESSION_IDLE: Duration = Duration::from_secs(15 * 60);
/// A script is stopped once it has printed this much (memory bound).
const CAPTURE_LIMIT: usize = 1024 * 1024;
/// Longest accepted protocol line; longer ones are discarded with an error.
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
/// Largest `code` argument accepted by `run_forge` / `check_forge`.
const MAX_CODE_BYTES: usize = 1024 * 1024;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
const RESOURCE_NOT_FOUND: i64 = -32002;
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CLIENT_CAPS: &str = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// The language guide served by `forge_reference`.
const REFERENCE: &str = include_str!("../../llms.txt");

/// What scripts may do and how long they may run.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Policy every script runs under. Start from
    /// [`Capabilities::deny_all`] and grant only what the agent needs.
    pub capabilities: Capabilities,
    /// Wall-clock limit per call; an agent's `timeout_secs` can only lower it.
    pub max_time: Duration,
    /// Script output beyond this many bytes is cut from the response.
    pub max_response_bytes: usize,
    /// Calls beyond this many in flight are rejected as busy.
    pub max_concurrent_calls: usize,
    /// Live `run_forge` sessions at most; 0 disables sessions (and the
    /// `session_id` argument and `reset_session` tool).
    pub max_sessions: usize,
    /// A session unused for this long is dropped.
    pub session_idle: Duration,
    /// Serve `run_forge`, `check_forge`, `forge_reference` and
    /// `reset_session`. On by default; `forge mcp serve` turns it off
    /// unless asked, so a tool server only exposes its own tools.
    pub code_tools: bool,
    /// Tools and resources written in Forge (`forge mcp serve`).
    pub tools: Option<Arc<ToolSet>>,
}

/// Names of the built-in tools (reserved when `code_tools` is on).
pub const CODE_TOOLS: &[&str] = &[
    "run_forge",
    "check_forge",
    "forge_reference",
    "reset_session",
];

impl ServerConfig {
    pub fn new(capabilities: Capabilities) -> Self {
        ServerConfig {
            capabilities,
            max_time: DEFAULT_MAX_TIME,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_concurrent_calls: DEFAULT_MAX_CONCURRENT_CALLS,
            max_sessions: DEFAULT_MAX_SESSIONS,
            session_idle: DEFAULT_SESSION_IDLE,
            code_tools: true,
            tools: None,
        }
    }

    /// Serve `tools` too. Fails if a Forge tool would shadow a built-in one.
    pub fn with_tools(mut self, tools: ToolSet) -> Result<Self, String> {
        if self.code_tools {
            if let Some(name) = tools.tool_names().find(|n| CODE_TOOLS.contains(n)) {
                return Err(format!(
                    "{}: tool name `{}` is reserved by forge mcp (rename it with @tool(name: ...))",
                    tools.path(),
                    name
                ));
            }
        }
        self.tools = Some(Arc::new(tools));
        Ok(self)
    }

    fn sessions_enabled(&self) -> bool {
        self.code_tools && self.max_sessions > 0
    }

    /// One-line description of the policy for agents and logs.
    pub fn policy_summary(&self) -> String {
        let granted = self.capabilities.describe();
        let granted = if granted.is_empty() {
            "nothing (pure computation only: no files, network, environment, \
             databases, subprocesses or AI calls)"
                .to_string()
        } else {
            format!("{}; everything else is denied", granted.join(", "))
        };
        format!(
            "Granted: {}. Time limit: {}s per call.",
            granted,
            fmt_secs(self.max_time)
        )
    }
}

fn fmt_secs(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s.fract() == 0.0 {
        format!("{}", s as u64)
    } else {
        format!("{:.3}", s)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

/// Serve MCP on the process's stdin/stdout until stdin closes.
///
/// On Unix the protocol is first moved to private (close-on-exec) copies of
/// fds 0 and 1; fd 0 is then pointed at `/dev/null` and fd 1 at stderr, so
/// `input()`, `print` paths that bypass the sandbox capture, and
/// subprocesses can never read or write the protocol stream.
pub fn serve_stdio(config: ServerConfig) -> io::Result<()> {
    let (input, output) = take_stdio()?;
    serve(io::BufReader::new(input), output, config)
}

#[cfg(unix)]
fn take_stdio() -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
    use std::fs::File;
    use std::os::fd::FromRawFd;
    io::stdout().flush()?;
    // SAFETY: plain fd syscalls on fds this process owns. The duplicated
    // fds are fresh, so `File::from_raw_fd` takes sole ownership of them.
    unsafe {
        let proto_in = libc::fcntl(0, libc::F_DUPFD_CLOEXEC, 3);
        if proto_in < 0 {
            return Err(io::Error::last_os_error());
        }
        let proto_out = libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 3);
        if proto_out < 0 {
            let err = io::Error::last_os_error();
            libc::close(proto_in);
            return Err(err);
        }
        let null_in = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if null_in >= 0 {
            libc::dup2(null_in, 0);
            libc::close(null_in);
        }
        if libc::dup2(2, 1) < 0 {
            // No usable stderr: discard stray output instead.
            let null_out = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if null_out >= 0 {
                libc::dup2(null_out, 1);
                libc::close(null_out);
            }
        }
        Ok((
            Box::new(File::from_raw_fd(proto_in)),
            Box::new(File::from_raw_fd(proto_out)),
        ))
    }
}

#[cfg(not(unix))]
fn take_stdio() -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
    // Script output is still captured by the sandbox; only builtins that
    // bypass the capture (and `input()`) share the process streams here.
    Ok((Box::new(io::stdin()), Box::new(io::stdout())))
}

struct Server {
    config: ServerConfig,
    out: Mutex<Box<dyn Write + Send>>,
    /// Running tool calls by request id (JSON text), for cancellation.
    inflight: Mutex<HashMap<String, InFlight>>,
    idle: Condvar,
    sessions: Mutex<Sessions>,
}

/// A running call.
struct InFlight {
    /// Stops the call (client cancellation, or a reset of its session).
    stop: CancelHandle,
    /// Set only by `notifications/cancelled`: such a request gets no
    /// response. A call stopped for another reason still gets one.
    client_cancelled: CancelHandle,
}

/// Serve MCP over any line-oriented byte stream until `input` reaches EOF.
/// Calls still running at EOF are allowed to finish (each is bounded by the
/// configured time limit) and their responses are written before returning.
pub fn serve<R: BufRead>(
    mut input: R,
    output: impl Write + Send + 'static,
    config: ServerConfig,
) -> io::Result<()> {
    let server = Arc::new(Server {
        config,
        out: Mutex::new(Box::new(output)),
        inflight: Mutex::new(HashMap::new()),
        idle: Condvar::new(),
        sessions: Mutex::new(Sessions::default()),
    });
    let mut line = Vec::new();
    let result = loop {
        line.clear();
        match read_line_bounded(&mut input, &mut line, MAX_LINE_BYTES) {
            Ok(None) => break Ok(()),
            Ok(Some(false)) => server.send(&error_response(
                Value::Null,
                INVALID_REQUEST,
                &format!("message longer than {} bytes", MAX_LINE_BYTES),
                None,
            )),
            Ok(Some(true)) => {
                let text = line.trim_ascii();
                if !text.is_empty() {
                    handle_message(&server, text);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => break Err(e),
        }
    };
    server.wait_idle();
    result
}

/// Read one `\n`-terminated line of at most `max` bytes into `buf`.
/// `None` at EOF; `Some(false)` when the line was too long (it is skipped).
fn read_line_bounded<R: BufRead>(
    r: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> io::Result<Option<bool>> {
    let n = r.by_ref().take(max as u64 + 1).read_until(b'\n', buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') || buf.len() <= max {
        return Ok(Some(true));
    }
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            break;
        }
        match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => {
                r.consume(i + 1);
                break;
            }
            None => {
                let len = chunk.len();
                r.consume(len);
            }
        }
    }
    Ok(Some(false))
}

impl Server {
    /// Write one message as a single line. Write errors mean the client is
    /// gone; the read loop will see EOF, so they are ignored here.
    fn send(&self, msg: &Value) {
        let mut text = match serde_json::to_string(msg) {
            Ok(t) => t,
            Err(_) => return,
        };
        text.push('\n');
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = out.write_all(text.as_bytes());
        let _ = out.flush();
    }

    fn wait_idle(&self) {
        let mut inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        while !inflight.is_empty() {
            inflight = self.idle.wait(inflight).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn server_info() -> Value {
        json!({
            "name": "forge",
            "title": "Forge sandbox",
            "version": env!("CARGO_PKG_VERSION"),
        })
    }

    fn capabilities(&self) -> Value {
        let mut caps = json!({ "tools": { "listChanged": false } });
        if self
            .config
            .tools
            .as_ref()
            .is_some_and(|t| t.has_resources())
        {
            caps["resources"] = json!({ "listChanged": false, "subscribe": false });
        }
        caps
    }

    fn instructions(&self) -> String {
        let mut text = String::new();
        if let Some(tools) = &self.config.tools {
            text.push_str(&format!(
                "Tools written in Forge ({}): {}. ",
                tools.path(),
                tools.tool_names().collect::<Vec<_>>().join(", ")
            ));
        }
        if self.config.code_tools {
            text.push_str(
                "Run Forge scripts in a sandbox. Instead of chaining many tool calls, write one \
                 short Forge program that does the work (HTTP, JSON, CSV, math, strings, \
                 collections are built in) and pass it to run_forge; print results with `say` \
                 or `println`. If you do not know Forge, call forge_reference first; use \
                 check_forge to find syntax and type errors without running. ",
            );
            if self.config.sessions_enabled() {
                text.push_str(
                    "Pass the same session_id to run_forge to keep variables and functions \
                     between calls; reset_session clears one. ",
                );
            }
        }
        text.push_str(&self.config.policy_summary());
        text
    }
}

fn error_response(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Which protocol era a request belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Era {
    Legacy,
    Modern,
}

/// Classify a request by its `_meta` (absent → legacy). Modern requests
/// must name a supported version and declare client capabilities.
fn request_era(params: &Map<String, Value>) -> Result<Era, (i64, String, Option<Value>)> {
    let meta = params.get("_meta").and_then(Value::as_object);
    let Some(version) = meta.and_then(|m| m.get(META_VERSION)) else {
        return Ok(Era::Legacy);
    };
    let Some(version) = version.as_str() else {
        return Err((
            INVALID_PARAMS,
            format!("_meta.{} must be a string", META_VERSION),
            None,
        ));
    };
    if MODERN_VERSIONS.contains(&version) {
        if !meta.is_some_and(|m| m.get(META_CLIENT_CAPS).is_some_and(Value::is_object)) {
            return Err((
                INVALID_PARAMS,
                format!("_meta.{} is required", META_CLIENT_CAPS),
                None,
            ));
        }
        Ok(Era::Modern)
    } else if LEGACY_VERSIONS.contains(&version) {
        Ok(Era::Legacy)
    } else {
        Err((
            UNSUPPORTED_PROTOCOL_VERSION,
            "Unsupported protocol version".to_string(),
            Some(json!({ "supported": supported_versions(), "requested": version })),
        ))
    }
}

fn supported_versions() -> Vec<&'static str> {
    MODERN_VERSIONS
        .iter()
        .chain(LEGACY_VERSIONS.iter())
        .copied()
        .collect()
}

/// Add the fields every modern result carries.
fn finish(mut result: Value, era: Era) -> Value {
    if era == Era::Modern {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("resultType".into(), json!("complete"));
            let meta = obj.entry("_meta").or_insert_with(|| json!({}));
            if let Some(meta) = meta.as_object_mut() {
                meta.insert(META_SERVER_INFO.into(), Server::server_info());
            }
        }
    }
    result
}

fn handle_message(server: &Arc<Server>, text: &[u8]) {
    let msg: Value = match serde_json::from_slice(text) {
        Ok(v) => v,
        Err(e) => {
            server.send(&error_response(
                Value::Null,
                PARSE_ERROR,
                &format!("Parse error: {}", e),
                None,
            ));
            return;
        }
    };
    let Some(obj) = msg.as_object() else {
        let why = if msg.is_array() {
            "JSON-RPC batches are not supported by MCP"
        } else {
            "a JSON-RPC message must be an object"
        };
        server.send(&error_response(Value::Null, INVALID_REQUEST, why, None));
        return;
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method") else {
        // A response to a request we never send: nothing to do.
        if id.is_some() && (obj.contains_key("result") || obj.contains_key("error")) {
            return;
        }
        server.send(&error_response(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "missing method",
            None,
        ));
        return;
    };
    let reply_id = match &id {
        Some(v @ (Value::String(_) | Value::Number(_))) => Some(v.clone()),
        Some(_) => {
            server.send(&error_response(
                Value::Null,
                INVALID_REQUEST,
                "id must be a string or a number",
                None,
            ));
            return;
        }
        None => None,
    };
    let invalid = |message: &str| {
        if let Some(id) = &reply_id {
            server.send(&error_response(id.clone(), INVALID_REQUEST, message, None));
        }
    };
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        invalid("jsonrpc must be \"2.0\"");
        return;
    }
    let Some(method) = method.as_str() else {
        invalid("method must be a string");
        return;
    };
    let params = match obj.get("params") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(p)) => p.clone(),
        Some(_) => {
            if let Some(id) = &reply_id {
                server.send(&error_response(
                    id.clone(),
                    INVALID_PARAMS,
                    "params must be an object",
                    None,
                ));
            }
            return;
        }
    };
    match reply_id {
        None => handle_notification(server, method, &params),
        Some(id) => handle_request(server, id, method, params),
    }
}

fn handle_notification(server: &Server, method: &str, params: &Map<String, Value>) {
    if method == "notifications/cancelled" {
        if let Some(id) = params.get("requestId") {
            let key = id.to_string();
            let inflight = server.inflight.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(call) = inflight.get(&key) {
                call.client_cancelled.cancel();
                call.stop.cancel();
            }
        }
    }
    // notifications/initialized and anything else: nothing to do.
}

fn handle_request(server: &Arc<Server>, id: Value, method: &str, params: Map<String, Value>) {
    if method == "initialize" {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = requested
            .filter(|v| LEGACY_VERSIONS.contains(v))
            .unwrap_or(LEGACY_VERSIONS[0]);
        server.send(&result_response(
            id,
            json!({
                "protocolVersion": version,
                "capabilities": server.capabilities(),
                "serverInfo": Server::server_info(),
                "instructions": server.instructions(),
            }),
        ));
        return;
    }
    let era = match request_era(&params) {
        Ok(era) => era,
        Err((code, message, data)) => {
            server.send(&error_response(id, code, &message, data));
            return;
        }
    };
    let result = match method {
        "ping" => json!({}),
        "server/discover" => json!({
            "supportedVersions": supported_versions(),
            "capabilities": server.capabilities(),
            "instructions": server.instructions(),
            "ttlMs": 3_600_000,
            "cacheScope": "public",
        }),
        "tools/list" => {
            let mut result = json!({ "tools": tool_definitions(&server.config) });
            if era == Era::Modern {
                result["ttlMs"] = json!(3_600_000);
                result["cacheScope"] = json!("public");
            }
            result
        }
        "tools/call" => {
            start_tool_call(server, id, era, params);
            return;
        }
        "resources/list" => {
            let resources = server
                .config
                .tools
                .as_ref()
                .map(|t| t.resource_definitions())
                .unwrap_or_default();
            json!({ "resources": resources })
        }
        "resources/templates/list" => json!({ "resourceTemplates": [] }),
        "resources/read" => {
            start_resource_read(server, id, era, params);
            return;
        }
        _ => {
            server.send(&error_response(
                id,
                METHOD_NOT_FOUND,
                &format!("Method not found: {}", method),
                None,
            ));
            return;
        }
    };
    server.send(&result_response(id, finish(result, era)));
}

fn tool_definitions(config: &ServerConfig) -> Value {
    let mut tools = Vec::new();
    if config.code_tools {
        tools.extend(code_tool_definitions(config));
    }
    if let Some(set) = &config.tools {
        tools.extend(set.tool_definitions(config.max_time));
    }
    Value::Array(tools)
}

/// The built-in tools: `run_forge`, `check_forge`, `forge_reference` and,
/// with sessions on, `reset_session`.
fn code_tool_definitions(config: &ServerConfig) -> Vec<Value> {
    use crate::permissions::Capability;
    let caps = &config.capabilities;
    let writes = [Capability::Write, Capability::Db, Capability::Run]
        .iter()
        .any(|c| caps.is_granted(*c));
    let open_world = [Capability::Net, Capability::Ai, Capability::Run]
        .iter()
        .any(|c| caps.is_granted(*c));
    let tools = json!([
        {
            "name": "run_forge",
            "title": "Run Forge code",
            "description": format!(
                "Run a Forge program in a sandbox and return what it printed (say/println/print). \
                 Use it to do multi-step work in one call: fetch, transform, compute, then print \
                 the result. Runtime errors, permission denials and timeouts come back as tool \
                 errors with the output printed so far. {} Call forge_reference to learn the \
                 language.",
                config.policy_summary()
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Forge source code to run."
                    },
                    "session_id": {
                        "type": "string",
                        "pattern": "^[A-Za-z0-9_.:-]{1,128}$",
                        "description": format!(
                            "Optional. Run in this persistent session: top-level variables, \
                             functions and types from earlier calls with the same id stay \
                             defined. Omit for a fresh, stateless run. At most {} sessions; \
                             one idle for {}s is dropped.",
                            config.max_sessions,
                            fmt_secs(config.session_idle)
                        )
                    },
                    "timeout_secs": {
                        "type": "number",
                        "exclusiveMinimum": 0,
                        "description": format!(
                            "Wall-clock limit in seconds (at most {}, the default).",
                            fmt_secs(config.max_time)
                        )
                    }
                },
                "required": ["code"],
                "additionalProperties": false
            },
            "outputSchema": {
                "type": "object",
                "properties": {
                    "ok": { "type": "boolean" },
                    "stdout": { "type": "string" },
                    "truncated": { "type": "boolean" },
                    "elapsed_ms": { "type": "number" },
                    "session": {
                        "type": "object",
                        "description": "Present when session_id was given.",
                        "properties": {
                            "id": { "type": "string" },
                            "created": { "type": "boolean", "description": "This call started the session." },
                            "lost": { "type": "boolean", "description": "The session ended with this call (its state is gone)." }
                        },
                        "required": ["id", "created", "lost"]
                    },
                    "error": {
                        "type": ["object", "null"],
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": ["syntax", "permission_denied", "runtime", "timeout",
                                         "output_limit", "cancelled", "busy", "session"]
                            },
                            "message": { "type": "string" },
                            "line": { "type": "integer" }
                        },
                        "required": ["kind", "message"]
                    }
                },
                "required": ["ok", "stdout", "truncated", "elapsed_ms", "error"]
            },
            "annotations": {
                "readOnlyHint": !writes,
                "destructiveHint": writes,
                "idempotentHint": false,
                "openWorldHint": open_world
            }
        },
        {
            "name": "check_forge",
            "title": "Check Forge code",
            "description": "Parse and type-check a Forge program without running it. Returns \
                            syntax errors and type diagnostics with line numbers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Forge source code to check." }
                },
                "required": ["code"],
                "additionalProperties": false
            },
            "outputSchema": {
                "type": "object",
                "properties": {
                    "ok": { "type": "boolean" },
                    "diagnostics": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "line": { "type": "integer" },
                                "column": { "type": "integer" },
                                "severity": { "type": "string", "enum": ["error", "warning"] },
                                "code": { "type": "string", "description": "Type-checker diagnostic code (T0001...), absent for syntax errors" },
                                "message": { "type": "string" }
                            },
                            "required": ["line", "column", "severity", "message"]
                        }
                    }
                },
                "required": ["ok", "diagnostics"]
            },
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true,
                "openWorldHint": false
            }
        },
        {
            "name": "forge_reference",
            "title": "Forge language reference",
            "description": "The compact Forge language guide for writing correct code: syntax, \
                            builtins, standard library and idioms. Read it before writing \
                            Forge for the first time.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }
    ]);
    let mut tools = match tools {
        Value::Array(tools) => tools,
        _ => Vec::new(),
    };
    if config.sessions_enabled() {
        tools.push(json!({
            "name": "reset_session",
            "title": "Reset a run_forge session",
            "description": "Forget a run_forge session: its variables and functions are dropped \
                            and a call still running in it is stopped. The next run_forge with \
                            that session_id starts fresh.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The session to reset." }
                },
                "required": ["session_id"],
                "additionalProperties": false
            },
            "outputSchema": {
                "type": "object",
                "properties": { "existed": { "type": "boolean" } },
                "required": ["existed"]
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }));
    } else if let Some(run) = tools.first_mut() {
        if let Some(props) = run["inputSchema"]["properties"].as_object_mut() {
            props.remove("session_id");
        }
        if let Some(props) = run["outputSchema"]["properties"].as_object_mut() {
            props.remove("session");
        }
    }
    tools
}

/// A tool result the agent sees as a failure (`isError: true`).
fn tool_error(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

fn start_tool_call(server: &Arc<Server>, id: Value, era: Era, params: Map<String, Value>) {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        server.send(&error_response(
            id,
            INVALID_PARAMS,
            "tools/call needs a tool name",
            None,
        ));
        return;
    };
    let args = match params.get("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(a)) => a.clone(),
        Some(_) => {
            server.send(&result_response(
                id,
                finish(tool_error("arguments must be an object"), era),
            ));
            return;
        }
    };
    let config = &server.config;
    let code_tools = config.code_tools;
    match name {
        "forge_reference" if code_tools => {
            let sessions = if config.sessions_enabled() {
                " With `session_id`, run_forge keeps top-level variables, functions and \
                 types between calls that pass the same id (a failed call keeps what it \
                 defined before the error; tasks it spawned stop when it ends); \
                 reset_session forgets one."
            } else {
                ""
            };
            let text = format!(
                "{}\n## This server (forge mcp)\n\nrun_forge runs code on the tree-walking \
                 interpreter inside a sandbox. {} A denied operation fails with \
                 `permission denied: <capability>`; do not retry it. HTTP servers, `schedule` \
                 and `watch` blocks are not started, and `input()` reads nothing.{}\n",
                REFERENCE,
                config.policy_summary(),
                sessions
            );
            let result = json!({ "content": [{ "type": "text", "text": text }] });
            server.send(&result_response(id, finish(result, era)));
        }
        "run_forge" if code_tools => spawn_call(server, id, era, Job::Run(args)),
        "check_forge" if code_tools => spawn_call(server, id, era, Job::Check(args)),
        "reset_session" if config.sessions_enabled() => {
            let result = match args.get("session_id").and_then(Value::as_str) {
                Some(sid) => {
                    let existed = server
                        .sessions
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .reset(sid);
                    let text = if existed {
                        format!("Session `{}` reset.", sid)
                    } else {
                        format!("There was no session `{}`.", sid)
                    };
                    json!({
                        "content": [{ "type": "text", "text": text }],
                        "structuredContent": { "existed": existed }
                    })
                }
                None => tool_error("`session_id` (a string) is required"),
            };
            server.send(&result_response(id, finish(result, era)));
        }
        other if config.tools.as_ref().is_some_and(|t| t.has_tool(other)) => spawn_call(
            server,
            id,
            era,
            Job::Tool {
                name: other.to_string(),
                args,
            },
        ),
        other => server.send(&error_response(
            id,
            INVALID_PARAMS,
            &format!("Unknown tool: {}", other),
            None,
        )),
    }
}

fn start_resource_read(server: &Arc<Server>, id: Value, era: Era, params: Map<String, Value>) {
    let Some(uri) = params.get("uri").and_then(Value::as_str) else {
        server.send(&error_response(
            id,
            INVALID_PARAMS,
            "resources/read needs a uri",
            None,
        ));
        return;
    };
    if !server
        .config
        .tools
        .as_ref()
        .is_some_and(|t| t.has_resource(uri))
    {
        server.send(&error_response(
            id,
            RESOURCE_NOT_FOUND,
            "Resource not found",
            Some(json!({ "uri": uri })),
        ));
        return;
    }
    let uri = uri.to_string();
    spawn_call(server, id, era, Job::Resource { uri });
}

/// Work that runs on its own thread, bounded by the server's limits.
enum Job {
    Run(Map<String, Value>),
    Check(Map<String, Value>),
    Tool {
        name: String,
        args: Map<String, Value>,
    },
    Resource {
        uri: String,
    },
}

impl Job {
    /// Answer for a job that cannot start (busy server, no thread):
    /// a tool error for tool calls, a JSON-RPC error for resource reads.
    fn refusal(&self, message: &str, kind: &str) -> Result<Value, (i64, String)> {
        match self {
            Job::Resource { .. } => Err((INTERNAL_ERROR, message.to_string())),
            Job::Run(_) => {
                let mut result = tool_error(message);
                result["structuredContent"] = json!({
                    "ok": false, "stdout": "", "truncated": false, "elapsed_ms": 0,
                    "error": { "kind": kind, "message": message }
                });
                Ok(result)
            }
            _ => Ok(tool_error(message)),
        }
    }

    fn run(self, server: &Server, cancel: &CancelHandle) -> Result<Value, (i64, String)> {
        let config = &server.config;
        match self {
            Job::Run(args) => Ok(run_forge(server, &args, cancel)),
            Job::Check(args) => Ok(check_forge(&args)),
            Job::Tool { name, args } => Ok(match &config.tools {
                Some(set) => set.call(
                    &name,
                    &args,
                    config.max_time,
                    config.max_response_bytes,
                    cancel,
                ),
                None => tool_error("no Forge tools are loaded"),
            }),
            Job::Resource { uri } => match &config.tools {
                Some(set) => set
                    .read_resource(&uri, config.max_time, config.max_response_bytes, cancel)
                    .map_err(|e| (INTERNAL_ERROR, e)),
                None => Err((RESOURCE_NOT_FOUND, "Resource not found".to_string())),
            },
        }
    }
}

fn spawn_call(server: &Arc<Server>, id: Value, era: Era, job: Job) {
    let reply = |result: Result<Value, (i64, String)>| match result {
        Ok(result) => result_response(id.clone(), finish(result, era)),
        Err((code, message)) => error_response(id.clone(), code, &message, None),
    };
    let key = id.to_string();
    let handle = CancelHandle::new();
    let client_cancelled = CancelHandle::new();
    {
        let mut inflight = server.inflight.lock().unwrap_or_else(|e| e.into_inner());
        if inflight.contains_key(&key) {
            drop(inflight);
            server.send(&error_response(
                id,
                INVALID_REQUEST,
                "a request with this id is already running",
                None,
            ));
            return;
        }
        if inflight.len() >= server.config.max_concurrent_calls {
            drop(inflight);
            let message = format!(
                "server busy: {} calls are already running; retry when one finishes",
                server.config.max_concurrent_calls
            );
            server.send(&reply(job.refusal(&message, "busy")));
            return;
        }
        inflight.insert(
            key.clone(),
            InFlight {
                stop: handle.clone(),
                client_cancelled: client_cancelled.clone(),
            },
        );
    }
    let is_resource = matches!(job, Job::Resource { .. });
    let worker_server = server.clone();
    let worker_handle = handle.clone();
    let worker_id = id.clone();
    let spawned = std::thread::Builder::new()
        .name("forge-mcp-call".to_string())
        .stack_size(crate::runtime::recursion::WORKER_STACK_SIZE)
        .spawn(move || {
            crate::runtime::recursion::register_thread_stack(
                crate::runtime::recursion::WORKER_STACK_SIZE,
            );
            let server = worker_server;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                job.run(&server, &worker_handle)
            }));
            let panicked = "internal error: the Forge runtime panicked";
            let result = outcome.unwrap_or_else(|_| {
                if is_resource {
                    Err((INTERNAL_ERROR, panicked.to_string()))
                } else {
                    Ok(tool_error(panicked))
                }
            });
            // Remove before replying so a client that sends the next call as
            // soon as it sees this response is never told the server is busy.
            {
                let mut inflight = server.inflight.lock().unwrap_or_else(|e| e.into_inner());
                inflight.remove(&key);
                // A cancelled request gets no response (MCP cancellation).
                if !client_cancelled.is_cancelled() {
                    server.send(&match result {
                        Ok(result) => result_response(worker_id, finish(result, era)),
                        Err((code, message)) => error_response(worker_id, code, &message, None),
                    });
                }
            }
            server.idle.notify_all();
        });
    if let Err(e) = spawned {
        server
            .inflight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id.to_string());
        server.idle.notify_all();
        let message = format!("cannot start a worker thread: {}", e);
        let result = if is_resource {
            Err((INTERNAL_ERROR, message))
        } else {
            Ok(tool_error(&message))
        };
        server.send(&reply(result));
    }
}

fn code_arg(args: &Map<String, Value>) -> Result<&str, Value> {
    match args.get("code") {
        Some(Value::String(code)) if code.len() > MAX_CODE_BYTES => Err(tool_error(&format!(
            "`code` is too large ({} bytes; the limit is {})",
            code.len(),
            MAX_CODE_BYTES
        ))),
        Some(Value::String(code)) => Ok(code),
        _ => Err(tool_error("`code` (a string of Forge source) is required")),
    }
}

/// A `run_forge` failure that happened before any code ran.
fn run_refused(kind: &str, message: &str) -> Value {
    let mut result = tool_error(message);
    result["structuredContent"] = json!({
        "ok": false, "stdout": "", "truncated": false, "elapsed_ms": 0,
        "error": { "kind": kind, "message": message }
    });
    result
}

/// Where a `run_forge` call ran, when it used a session.
struct SessionNote {
    id: String,
    created: bool,
    lost: bool,
}

/// Run `code` in session `id`: check the session out, run one step on its
/// interpreter under `sandbox`, and check it back in.
fn run_in_session(
    server: &Server,
    sandbox: &Sandbox,
    id: &str,
    code: &str,
    cancel: &CancelHandle,
) -> Result<(Result<Output, SandboxError>, SessionNote), Value> {
    let config = &server.config;
    let note = |created, lost| SessionNote {
        id: id.to_string(),
        created,
        lost,
    };
    // A syntax error never touches (or creates) the session.
    let program = match parse_source(code) {
        Ok(p) => p,
        Err(e) => return Ok((Err(e), note(false, false))),
    };
    let checkout = server
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .checkout(id, config.max_sessions, config.session_idle, cancel);
    let (mut interp, generation, created) = match checkout {
        Checkout::Ready {
            interp,
            generation,
            created,
        } => (*interp, generation, created),
        Checkout::Busy => {
            return Err(run_refused(
                "session",
                &format!(
                    "session `{}` is already running a call; wait for it to finish or use \
                     another session_id",
                    id
                ),
            ))
        }
        Checkout::Full(max) => {
            return Err(run_refused(
                "session",
                &format!(
                    "this server keeps at most {} sessions and all are in use; call \
                     reset_session on one you no longer need (idle sessions expire after {}s)",
                    max,
                    fmt_secs(config.session_idle)
                ),
            ))
        }
    };
    interp.source = Some(code.to_string());
    interp.source_file = Some("<run_forge>".into());
    let run = sandbox.run_interpreter(interp, cancel, move |interp| {
        interp.run(&program).map(|_| ())
    });
    let alive = server
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .checkin(id, generation, run.interp);
    let outcome = run.result.map(|((), stdout)| Output { stdout });
    Ok((outcome, note(created, !alive)))
}

fn run_forge(server: &Server, args: &Map<String, Value>, cancel: &CancelHandle) -> Value {
    let config = &server.config;
    let code = match code_arg(args) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let limit = match args.get("timeout_secs") {
        None | Some(Value::Null) => config.max_time,
        Some(v) => match v.as_f64() {
            Some(secs) if secs.is_finite() && secs > 0.0 => Duration::try_from_secs_f64(secs)
                .map_or(config.max_time, |d| d.min(config.max_time)),
            _ => return tool_error("`timeout_secs` must be a positive number of seconds"),
        },
    };
    let session_id = match args.get("session_id") {
        None | Some(Value::Null) => None,
        Some(_) if !config.sessions_enabled() => {
            return tool_error("this server does not keep sessions; omit `session_id`")
        }
        Some(Value::String(id)) if session::valid_session_id(id) => Some(id.as_str()),
        Some(_) => {
            return tool_error("`session_id` must be 1-128 letters, digits, `_`, `-`, `.` or `:`")
        }
    };
    let sandbox = Sandbox::with_capabilities(config.capabilities.clone())
        .max_time(limit)
        .max_output(CAPTURE_LIMIT.max(config.max_response_bytes))
        .source_label("<run_forge>");
    let started = Instant::now();
    let (outcome, session) = match session_id {
        None => (sandbox.run_source_cancellable(code, cancel), None),
        Some(id) => match run_in_session(server, &sandbox, id, code, cancel) {
            Ok((outcome, note)) => (outcome, Some(note)),
            Err(refused) => return refused,
        },
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let full = match &outcome {
        Ok(out) => out.stdout.clone(),
        Err(e) => e.stdout().to_string(),
    };
    let total = full.len();
    let truncated = total > config.max_response_bytes;
    let stdout = truncate_utf8(full, config.max_response_bytes);
    let note = truncated.then(|| {
        format!(
            "[output truncated: showing the first {} of {} bytes]",
            stdout.len(),
            total
        )
    });

    let (mut text, error) = match &outcome {
        Ok(_) => {
            let mut text = if stdout.is_empty() {
                "(no output)".to_string()
            } else {
                stdout.clone()
            };
            if let Some(note) = &note {
                text.push_str(&format!("\n{}", note));
            }
            (text, Value::Null)
        }
        Err(e) => {
            let mut text = e.to_string();
            match e {
                SandboxError::PermissionDenied { .. } => text.push_str(&format!(
                    "\nThis server's sandbox does not grant that. {}",
                    config.policy_summary()
                )),
                SandboxError::Timeout { .. } if limit < config.max_time => text.push_str(&format!(
                    " (the server allows up to {}s)",
                    fmt_secs(config.max_time)
                )),
                SandboxError::OutputLimit { .. } => {
                    text.push_str("; print less, or summarize before printing")
                }
                _ => {}
            }
            if !stdout.is_empty() {
                text.push_str("\n\nOutput before the error:\n");
                text.push_str(&stdout);
                if let Some(note) = &note {
                    text.push_str(&format!("\n{}", note));
                }
            }
            let mut error = json!({ "kind": e.kind(), "message": e.to_string() });
            if let SandboxError::Runtime { line, .. } = e {
                if *line > 0 {
                    error["line"] = json!(line);
                }
            }
            (text, error)
        }
    };
    if let Some(s) = session.as_ref().filter(|s| s.lost) {
        let why = if matches!(outcome, Err(SandboxError::Cancelled { .. })) {
            "it was reset while this call ran"
        } else {
            "the script did not stop in time, so its state is gone"
        };
        text.push_str(&format!(
            "\n[session `{}` ended: {}; the next call with this id starts a new session]",
            s.id, why
        ));
    }
    let ok = outcome.is_ok();
    let mut result = json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": {
            "ok": ok,
            "stdout": stdout,
            "truncated": truncated,
            "elapsed_ms": elapsed_ms,
            "error": error,
        }
    });
    if let Some(s) = session {
        result["structuredContent"]["session"] =
            json!({ "id": s.id, "created": s.created, "lost": s.lost });
    }
    if !ok {
        result["isError"] = json!(true);
    }
    result
}

/// One problem found by [`check_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line (0 when unknown).
    pub line: usize,
    /// 1-based column (0 when unknown).
    pub column: usize,
    pub is_error: bool,
    /// Type-checker code (`T0006`), `None` for lex/parse errors.
    pub code: Option<String>,
    pub message: String,
}

/// Lex, parse and type-check `source` without running it.
pub fn check_source(source: &str) -> Vec<Diagnostic> {
    use crate::typechecker::{analyze, CheckOptions, FrontendError};
    match analyze(source, &CheckOptions::default()) {
        Ok(analysis) => analysis
            .diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                line: d.line(),
                column: d.col(),
                is_error: d.is_error(),
                code: Some(d.code.as_str().to_string()),
                message: d.full_message(),
            })
            .collect(),
        Err(FrontendError::Lex { line, col, message })
        | Err(FrontendError::Parse { line, col, message }) => vec![Diagnostic {
            line,
            column: col,
            is_error: true,
            code: None,
            message,
        }],
    }
}

fn check_forge(args: &Map<String, Value>) -> Value {
    let code = match code_arg(args) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let diagnostics = check_source(code);
    let ok = !diagnostics.iter().any(|d| d.is_error);
    let text = if diagnostics.is_empty() {
        "No problems found.".to_string()
    } else {
        diagnostics
            .iter()
            .map(|d| {
                format!(
                    "line {}:{}: {}{}: {}",
                    d.line,
                    d.column,
                    if d.is_error { "error" } else { "warning" },
                    d.code
                        .as_deref()
                        .map(|c| format!("[{}]", c))
                        .unwrap_or_default(),
                    d.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let items: Vec<Value> = diagnostics
        .iter()
        .map(|d| {
            let mut item = json!({
                "line": d.line,
                "column": d.column,
                "severity": if d.is_error { "error" } else { "warning" },
                "message": d.message,
            });
            if let Some(code) = &d.code {
                item["code"] = json!(code);
            }
            item
        })
        .collect();
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": { "ok": ok, "diagnostics": items }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Write` whose bytes can be read back after `serve` returns.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn session(input: &str, config: ServerConfig) -> Vec<Value> {
        let out = Captured::default();
        serve(
            io::Cursor::new(input.as_bytes().to_vec()),
            out.clone(),
            config,
        )
        .expect("serve");
        let bytes = out.0.lock().expect("lock").clone();
        String::from_utf8(bytes)
            .expect("utf8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("every line is JSON"))
            .collect()
    }

    fn deny_all() -> ServerConfig {
        ServerConfig::new(Capabilities::deny_all())
    }

    fn by_id(msgs: &[Value], id: i64) -> Value {
        msgs.iter()
            .find(|m| m["id"] == json!(id))
            .cloned()
            .unwrap_or_else(|| panic!("no response for id {id}: {msgs:?}"))
    }

    #[test]
    fn malformed_input_gets_errors_not_a_crash() {
        let msgs = session(
            "{not json\n[]\n42\n{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"ping\"}\n\
             {\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"ping\"}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"nope\"}\n\
             {\"jsonrpc\":\"2.0\",\"method\":\"notifications/whatever\"}\n\
             {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{}}\n\
             \n{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"ping\"}\n",
            deny_all(),
        );
        let codes: Vec<i64> = msgs
            .iter()
            .filter_map(|m| m["error"]["code"].as_i64())
            .collect();
        assert_eq!(
            codes,
            vec![
                PARSE_ERROR,
                INVALID_REQUEST,
                INVALID_REQUEST,
                INVALID_REQUEST,
                INVALID_REQUEST,
                METHOD_NOT_FOUND
            ]
        );
        assert_eq!(by_id(&msgs, 4)["result"], json!({}));
        assert_eq!(msgs.len(), 7, "{msgs:?}");
    }

    #[test]
    fn legacy_initialize_negotiates_a_version() {
        let msgs = session(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"t\",\"version\":\"1\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"1999-01-01\"}}\n",
            deny_all(),
        );
        assert_eq!(by_id(&msgs, 1)["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            by_id(&msgs, 2)["result"]["protocolVersion"],
            LEGACY_VERSIONS[0]
        );
        let init = by_id(&msgs, 1)["result"].clone();
        assert!(init["capabilities"]["tools"].is_object());
        assert!(init["instructions"]
            .as_str()
            .is_some_and(|s| s.contains("Granted: nothing")));
        assert!(init.get("resultType").is_none());
    }

    #[test]
    fn modern_requests_are_stateless_and_versioned() {
        let meta = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}"#;
        let input = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server/discover\",\"params\":{{{meta}}}}}\n\
             {{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{{{meta}}}}}\n\
             {{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\",\"params\":{{\"_meta\":{{\"io.modelcontextprotocol/protocolVersion\":\"2099-01-01\"}}}}}}\n\
             {{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/list\",\"params\":{{\"_meta\":{{\"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\"}}}}}}\n\
             {{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{{\"name\":\"run_forge\",\"arguments\":{{\"code\":\"say 6 * 7\"}},{meta}}}}}\n"
        );
        let msgs = session(&input, deny_all());
        let discover = by_id(&msgs, 1)["result"].clone();
        assert_eq!(discover["resultType"], "complete");
        assert_eq!(discover["supportedVersions"][0], "2026-07-28");
        assert_eq!(discover["_meta"][META_SERVER_INFO]["name"], "forge");
        let list = by_id(&msgs, 2)["result"].clone();
        assert_eq!(list["cacheScope"], "public");
        let names: Vec<&str> = list["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "run_forge",
                "check_forge",
                "forge_reference",
                "reset_session"
            ]
        );
        let unsupported = by_id(&msgs, 3)["error"].clone();
        assert_eq!(unsupported["code"], UNSUPPORTED_PROTOCOL_VERSION);
        assert_eq!(unsupported["data"]["requested"], "2099-01-01");
        assert_eq!(by_id(&msgs, 4)["error"]["code"], INVALID_PARAMS);
        let call = by_id(&msgs, 5)["result"].clone();
        assert_eq!(call["resultType"], "complete");
        assert_eq!(call["structuredContent"]["stdout"], "42\n");
    }

    #[test]
    fn run_forge_results() {
        let call = |id: i64, args: &str| {
            format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"tools/call\",\"params\":{{\"name\":\"run_forge\",\"arguments\":{args}}}}}\n"
            )
        };
        let mut config = deny_all();
        config.max_response_bytes = 20;
        let input = [
            call(1, r#"{"code":"say \"hi\""}"#),
            call(
                2,
                r#"{"code":"say \"before\"\nfs.read(\"/etc/hostname\")"}"#,
            ),
            call(3, r#"{"code":"let x = 1 / 0"}"#),
            call(4, r#"{"code":"let = ="}"#),
            call(5, r#"{"code":"for i in range(0, 50) { say \"line\" }"}"#),
            call(6, r#"{}"#),
            call(7, r#"{"code":"say 1","timeout_secs":-1}"#),
            call(8, r#"{"code":""}"#),
        ]
        .concat();
        let msgs = session(&input, config);
        let r = |id| by_id(&msgs, id)["result"].clone();
        assert_eq!(r(1)["content"][0]["text"], "hi\n");
        assert!(r(1).get("isError").is_none());
        assert_eq!(r(1)["structuredContent"]["ok"], true);

        assert_eq!(r(2)["isError"], true);
        assert_eq!(
            r(2)["structuredContent"]["error"]["kind"],
            "permission_denied"
        );
        assert_eq!(r(2)["structuredContent"]["stdout"], "before\n");
        let text = r(2)["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string();
        assert!(text.starts_with("permission denied: fs.read"), "{text}");
        assert!(text.contains("Output before the error:\nbefore"), "{text}");

        assert_eq!(r(3)["structuredContent"]["error"]["kind"], "runtime");
        assert_eq!(r(3)["structuredContent"]["error"]["line"], 1);
        assert_eq!(r(4)["structuredContent"]["error"]["kind"], "syntax");

        assert_eq!(r(5)["structuredContent"]["truncated"], true);
        assert_eq!(
            r(5)["structuredContent"]["stdout"],
            "line\nline\nline\nline\n"
        );
        assert!(r(5)["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("[output truncated: showing the first 20 of 250 bytes]")));

        assert_eq!(r(6)["isError"], true);
        assert_eq!(r(7)["isError"], true);
        assert_eq!(r(8)["content"][0]["text"], "(no output)");
    }

    #[test]
    fn check_forge_and_reference() {
        let msgs = session(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"check_forge\",\"arguments\":{\"code\":\"let x = 1\\nlet y = (\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"check_forge\",\"arguments\":{\"code\":\"say 1\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"forge_reference\"}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"rm_rf\"}}\n",
            deny_all(),
        );
        let bad = by_id(&msgs, 1)["result"]["structuredContent"].clone();
        assert_eq!(bad["ok"], false);
        assert_eq!(bad["diagnostics"][0]["severity"], "error");
        assert_eq!(bad["diagnostics"][0]["line"], 2);
        let good = by_id(&msgs, 2)["result"].clone();
        assert_eq!(good["structuredContent"]["ok"], true);
        assert_eq!(good["content"][0]["text"], "No problems found.");
        let reference = by_id(&msgs, 3)["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string();
        assert!(reference.starts_with("# Forge"));
        assert!(reference.contains("## This server (forge mcp)"));
        assert_eq!(by_id(&msgs, 4)["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn busy_and_duplicate_ids() {
        let mut config = deny_all();
        config.max_concurrent_calls = 1;
        config.max_time = Duration::from_millis(500);
        let msgs = session(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"run_forge\",\"arguments\":{\"code\":\"while true { }\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"run_forge\",\"arguments\":{\"code\":\"say 1\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"run_forge\",\"arguments\":{\"code\":\"say 1\"}}}\n",
            config,
        );
        assert_eq!(
            by_id(&msgs, 2)["result"]["structuredContent"]["error"]["kind"],
            "busy"
        );
        let ones: Vec<&Value> = msgs.iter().filter(|m| m["id"] == json!(1)).collect();
        assert_eq!(ones.len(), 2, "{msgs:?}");
        assert!(ones.iter().any(|m| m["error"]["code"] == INVALID_REQUEST));
        assert!(ones
            .iter()
            .any(|m| m["result"]["structuredContent"]["error"]["kind"] == "timeout"));
    }

    #[test]
    fn cancelled_requests_get_no_response() {
        // The cancel notification arrives while the call runs; the server
        // drains at EOF, so the absence of a response is observable.
        let msgs = session(
            "{\"jsonrpc\":\"2.0\",\"id\":\"c1\",\"method\":\"tools/call\",\"params\":{\"name\":\"run_forge\",\"arguments\":{\"code\":\"while true { }\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":\"c1\",\"reason\":\"user\"}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n",
            deny_all(),
        );
        assert!(msgs.iter().all(|m| m["id"] != json!("c1")), "{msgs:?}");
        assert_eq!(by_id(&msgs, 2)["result"], json!({}));
    }

    #[test]
    fn overlong_lines_are_rejected_and_skipped() {
        let mut input = Vec::new();
        let mut reader = io::Cursor::new(b"aaaaaaaaaa\nok\n".to_vec());
        assert_eq!(
            read_line_bounded(&mut reader, &mut input, 4).ok(),
            Some(Some(false))
        );
        input.clear();
        assert_eq!(
            read_line_bounded(&mut reader, &mut input, 4).ok(),
            Some(Some(true))
        );
        assert_eq!(input, b"ok\n");
        input.clear();
        assert_eq!(
            read_line_bounded(&mut reader, &mut input, 4).ok(),
            Some(None)
        );
    }

    #[test]
    fn policy_summary_mentions_grants() {
        let config =
            ServerConfig::new(Capabilities::deny_all().grant_net_hosts(["api.example.com"]));
        let summary = config.policy_summary();
        assert!(summary.contains("net (api.example.com)"), "{summary}");
        assert!(summary.contains("Time limit: 30s"), "{summary}");
        let tools = tool_definitions(&config);
        assert_eq!(tools[0]["annotations"]["openWorldHint"], true);
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    }

    fn names(tools: &Value) -> Vec<String> {
        tools
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_string))
            .collect()
    }

    fn tool_set(src: &str) -> ToolSet {
        ToolSet::load(
            std::path::Path::new("t.fg"),
            src,
            Capabilities::deny_all(),
            Duration::from_secs(5),
        )
        .expect("loads")
        .0
    }

    #[test]
    fn sessions_can_be_disabled() {
        let mut config = deny_all();
        config.max_sessions = 0;
        let tools = tool_definitions(&config);
        assert_eq!(
            names(&tools),
            ["run_forge", "check_forge", "forge_reference"]
        );
        assert!(tools[0]["inputSchema"]["properties"]
            .get("session_id")
            .is_none());
        let msgs = session(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"run_forge\",\"arguments\":{\"code\":\"say 1\",\"session_id\":\"a\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"reset_session\",\"arguments\":{\"session_id\":\"a\"}}}\n",
            config,
        );
        assert_eq!(by_id(&msgs, 1)["result"]["isError"], true);
        assert_eq!(by_id(&msgs, 2)["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn forge_tools_are_listed_and_called() {
        // Built-in names are reserved while the code tools are served.
        assert!(deny_all()
            .with_tools(tool_set("@tool(\"d\")\nfn run_forge() {}"))
            .is_err());

        let mut config = deny_all();
        config.code_tools = false;
        let config = config
            .with_tools(tool_set(
                "@tool(\"Say hi\")\nfn hello(name: String) -> String { return \"hi \" + name }\n\
                 @resource(\"forge://n\")\nfn n() { return 42 }",
            ))
            .expect("no clash");
        assert_eq!(names(&tool_definitions(&config)), ["hello"]);
        let msgs = session(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"hello\",\"arguments\":{\"name\":\"Ada\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"run_forge\",\"arguments\":{\"code\":\"say 1\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"resources/read\",\"params\":{\"uri\":\"forge://n\"}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"resources/read\",\"params\":{\"uri\":\"forge://zzz\"}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\"}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"resources/list\"}\n",
            config,
        );
        let hello = by_id(&msgs, 1)["result"].clone();
        assert_eq!(hello["structuredContent"], json!({"result": "hi Ada"}));
        assert_eq!(hello["content"][0]["text"], "hi Ada");
        assert_eq!(by_id(&msgs, 2)["error"]["code"], INVALID_PARAMS);
        assert_eq!(by_id(&msgs, 3)["result"]["contents"][0]["text"], "42");
        assert_eq!(by_id(&msgs, 4)["error"]["code"], RESOURCE_NOT_FOUND);
        let init = by_id(&msgs, 5)["result"].clone();
        assert!(init["capabilities"]["resources"].is_object(), "{init}");
        assert!(init["instructions"]
            .as_str()
            .is_some_and(|s| s.contains("Tools written in Forge (t.fg): hello")));
        assert_eq!(
            by_id(&msgs, 6)["result"]["resources"][0]["uri"],
            "forge://n"
        );
    }
}
