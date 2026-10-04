#![allow(dead_code)]

//! Forge language server.
//!
//! Transport, framing and the initialize/shutdown/exit handshake are provided
//! by `lsp-server` (rust-analyzer's synchronous LSP scaffold). Capabilities
//! and method names come from `lsp-types`, so adding a new feature is:
//! advertise it in [`server_capabilities`], add a match arm in
//! [`handle_request`] (or [`handle_notification`]), and write the handler.
//!
//! Provides: diagnostics (lex/parse errors + type-check warnings),
//! completions, hover, go-to-definition, references, document symbols,
//! whole-document formatting and signature help.

use crate::parser::ast::Stmt;
use lsp_server::{Connection, ErrorCode, Message, Notification, ProtocolError, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use lsp_types::{
    notification, request, CompletionOptions, HoverProviderCapability, OneOf, ServerCapabilities,
    ServerInfo, SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory document store: uri -> text content.
/// Updated on didOpen/didChange, used by hover/diagnostics.
static DOCUMENTS: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn store_document(uri: &str, text: &str) {
    if let Ok(mut docs) = DOCUMENTS.lock() {
        docs.insert(uri.to_string(), text.to_string());
    }
}

fn get_document(uri: &str) -> Option<String> {
    DOCUMENTS.lock().ok()?.get(uri).cloned()
}

fn remove_document(uri: &str) {
    if let Ok(mut docs) = DOCUMENTS.lock() {
        docs.remove(uri);
    }
}

/// Entry point for `forge lsp`: serve the Language Server Protocol over
/// stdin/stdout until the client sends `exit` or closes stdin.
///
/// Exit status follows the LSP spec: 0 when `exit` follows `shutdown`,
/// 1 otherwise (including stdin EOF without a shutdown handshake).
pub fn run_lsp() {
    eprintln!("Forge LSP server started");
    let (connection, io_threads) = Connection::stdio();
    let clean = match serve(&connection) {
        Ok(clean) => clean,
        Err(e) => {
            eprintln!("forge lsp: {}", e);
            false
        }
    };
    // Dropping the connection closes the writer channel so the writer thread
    // flushes and exits; the reader thread exits on `exit` or stdin EOF.
    drop(connection);
    if let Err(e) = io_threads.join() {
        eprintln!("forge lsp: io error: {}", e);
    }
    if !clean {
        std::process::exit(1);
    }
}

/// The capabilities advertised in the `initialize` response.
pub(crate) fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                ..Default::default()
            },
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: Default::default(),
        }),
        ..Default::default()
    }
}

/// The full `InitializeResult` payload.
fn initialize_result() -> serde_json::Value {
    serde_json::json!({
        "capabilities": server_capabilities(),
        "serverInfo": ServerInfo {
            name: "forge-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        },
    })
}

/// Run the server on an established connection (stdio in production, an
/// in-memory pair in tests). Returns `Ok(true)` for a clean
/// `shutdown` + `exit`, `Ok(false)` when the client went away or sent
/// `exit` without `shutdown`.
pub(crate) fn serve(connection: &Connection) -> Result<bool, ProtocolError> {
    let (id, _params) = connection.initialize_start()?;
    connection.initialize_finish(id, initialize_result())?;

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if req.method == request::Shutdown::METHOD {
                    // Replies to `shutdown`, then waits for `exit`.
                    return match connection.handle_shutdown(&req) {
                        Ok(_) => Ok(true),
                        Err(e) => {
                            // The shutdown handshake itself succeeded; only
                            // the trailing `exit` was missing or malformed.
                            eprintln!("forge lsp: {}", e);
                            Ok(true)
                        }
                    };
                }
                let resp = handle_request(&req);
                if connection.sender.send(resp.into()).is_err() {
                    return Ok(false);
                }
            }
            Message::Notification(note) => {
                if note.method == notification::Exit::METHOD {
                    return Ok(false);
                }
                for out in handle_notification(&note) {
                    if connection.sender.send(out.into()).is_err() {
                        return Ok(false);
                    }
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(false)
}

fn param_str<'a>(params: &'a serde_json::Value, pointer: &str) -> Option<&'a str> {
    params.pointer(pointer).and_then(|v| v.as_str())
}

/// Extract `(uri, line, character)` from a TextDocumentPositionParams.
fn position_params(params: &serde_json::Value) -> Option<(&str, usize, usize)> {
    let uri = param_str(params, "/textDocument/uri")?;
    let line = params.pointer("/position/line")?.as_u64()? as usize;
    let character = params.pointer("/position/character")?.as_u64()? as usize;
    Some((uri, line, character))
}

fn invalid_params(req: &Request) -> Response {
    Response::new_err(
        req.id.clone(),
        ErrorCode::InvalidParams as i32,
        format!("invalid params for {}", req.method),
    )
}

/// Handle one request. Every request gets exactly one response.
pub(crate) fn handle_request(req: &Request) -> Response {
    let params = &req.params;
    let id = req.id.clone();
    let result: Option<serde_json::Value> = match req.method.as_str() {
        request::Initialize::METHOD => Some(initialize_result()),
        request::Shutdown::METHOD => Some(serde_json::Value::Null),
        request::Completion::METHOD => {
            let context = param_str(params, "/context/triggerCharacter").unwrap_or("");
            let completions = if context == "." {
                get_module_completions(params)
            } else {
                get_completions()
            };
            Some(serde_json::json!(completions))
        }
        request::HoverRequest::METHOD => {
            position_params(params).map(|(uri, line, ch)| get_hover(uri, line, ch))
        }
        request::GotoDefinition::METHOD => {
            position_params(params).map(|(uri, line, ch)| get_definition(uri, line, ch))
        }
        request::References::METHOD => position_params(params)
            .map(|(uri, line, ch)| serde_json::json!(get_references(uri, line, ch))),
        request::DocumentSymbolRequest::METHOD => param_str(params, "/textDocument/uri")
            .map(|uri| serde_json::json!(get_document_symbols(uri))),
        request::Formatting::METHOD => {
            param_str(params, "/textDocument/uri").map(get_formatting_edits)
        }
        request::SignatureHelpRequest::METHOD => {
            position_params(params).map(|(uri, line, ch)| get_signature_help(uri, line, ch))
        }
        // Per LSP spec, requests for unhandled methods must return a
        // JSON-RPC MethodNotFound error rather than being dropped.
        other => {
            return Response::new_err(
                id,
                ErrorCode::MethodNotFound as i32,
                format!("method not found: {}", other),
            )
        }
    };
    match result {
        Some(value) => Response::new_ok(id, value),
        None => invalid_params(req),
    }
}

fn publish_diagnostics(uri: &str, diagnostics: Vec<serde_json::Value>) -> Notification {
    Notification::new(
        notification::PublishDiagnostics::METHOD.to_string(),
        serde_json::json!({ "uri": uri, "diagnostics": diagnostics }),
    )
}

/// Handle one notification, returning any notifications to send back
/// (e.g. `textDocument/publishDiagnostics`). Unknown notifications are
/// ignored, as the spec requires.
pub(crate) fn handle_notification(note: &Notification) -> Vec<Notification> {
    let params = &note.params;
    match note.method.as_str() {
        notification::DidOpenTextDocument::METHOD => {
            let (Some(uri), Some(text)) = (
                param_str(params, "/textDocument/uri"),
                param_str(params, "/textDocument/text"),
            ) else {
                return vec![];
            };
            store_document(uri, text);
            vec![publish_diagnostics(uri, get_diagnostics(text))]
        }
        notification::DidChangeTextDocument::METHOD => {
            let Some(uri) = param_str(params, "/textDocument/uri") else {
                return vec![];
            };
            // Full sync: the last change carries the whole document.
            let Some(text) = params
                .get("contentChanges")
                .and_then(|c| c.as_array())
                .and_then(|changes| changes.last())
                .and_then(|change| change.get("text"))
                .and_then(|t| t.as_str())
            else {
                return vec![];
            };
            store_document(uri, text);
            vec![publish_diagnostics(uri, get_diagnostics(text))]
        }
        notification::DidCloseTextDocument::METHOD => {
            let Some(uri) = param_str(params, "/textDocument/uri") else {
                return vec![];
            };
            remove_document(uri);
            vec![publish_diagnostics(uri, vec![])]
        }
        _ => vec![],
    }
}

/// Handle a single raw JSON-RPC message and return the first message to
/// send back, serialized. Convenience wrapper over [`handle_request`] /
/// [`handle_notification`] used by unit tests.
fn handle_message(body: &str) -> Option<String> {
    let msg: Message = serde_json::from_str(body).ok()?;
    let out: Message = match msg {
        Message::Request(req) => handle_request(&req).into(),
        Message::Notification(note) => handle_notification(&note).into_iter().next()?.into(),
        Message::Response(_) => return None,
    };
    serde_json::to_string(&out).ok()
}

/// Number of UTF-16 code units in `s` (LSP's default position encoding).
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Whole-document formatting via `forge fmt`'s formatter. Returns a single
/// TextEdit replacing the document, or no edits when already formatted.
fn get_formatting_edits(uri: &str) -> serde_json::Value {
    let Some(text) = get_document(uri).or_else(|| read_document(uri)) else {
        return serde_json::json!([]);
    };
    let formatted = crate::formatter::format_source(&text);
    if formatted == text {
        return serde_json::json!([]);
    }
    let end_line = text.matches('\n').count();
    let last_line = text.rsplit('\n').next().unwrap_or("");
    serde_json::json!([{
        "range": {
            "start": {"line": 0, "character": 0},
            "end": {"line": end_line, "character": utf16_len(last_line)}
        },
        "newText": formatted
    }])
}

/// Locate the innermost unclosed call on the cursor's line.
/// Returns `(callee_name, active_parameter_index)`.
fn find_enclosing_call(line_text: &str, character: usize) -> Option<(String, usize)> {
    let prefix: Vec<char> = line_text.chars().take(character).collect();
    // Track (open_paren_index, comma_count) for each unclosed '('.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut in_string: Option<char> = None;
    let mut i = 0;
    while i < prefix.len() {
        let c = prefix[i];
        if let Some(q) = in_string {
            if c == '\\' {
                i += 1;
            } else if c == q {
                in_string = None;
            }
        } else {
            match c {
                '"' | '\'' => in_string = Some(c),
                '/' if prefix.get(i + 1) == Some(&'/') => break,
                '(' | '[' | '{' => stack.push((i, 0)),
                ')' | ']' | '}' => {
                    stack.pop();
                }
                ',' => {
                    if let Some(top) = stack.last_mut() {
                        top.1 += 1;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    let &(open, commas) = stack.last()?;
    if prefix[open] != '(' {
        return None;
    }
    let mut end = open;
    while end > 0 && prefix[end - 1] == ' ' {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (prefix[start - 1].is_alphanumeric() || prefix[start - 1] == '_') {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some((prefix[start..end].iter().collect(), commas))
}

/// Split the parameter list out of a `fn name(a, b) -> T` signature.
fn signature_params(signature: &str) -> Vec<String> {
    let (Some(open), Some(close)) = (signature.find('('), signature.rfind(')')) else {
        return vec![];
    };
    if close <= open + 1 {
        return vec![];
    }
    signature[open + 1..close]
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Signature of a user-defined function. Uses the AST when the document
/// parses; while the user is mid-edit (the usual case for signature help)
/// the document often does not parse, so fall back to a textual scan for
/// `fn name(...)` / `define name(...)`.
fn user_fn_signature(source: &str, name: &str) -> Option<String> {
    if let Some(hover) = get_user_symbol_hover(source, name) {
        let sig = hover.trim_start_matches("```forge\n").lines().next()?;
        if sig.contains(&format!("fn {}(", name)) {
            return Some(sig.to_string());
        }
    }
    for line in source.lines() {
        let trimmed = line.trim_start();
        for kw in ["fn", "async fn", "define", "forge"] {
            let head = format!("{} {}(", kw, name);
            if let Some(rest) = trimmed.strip_prefix(&head) {
                let close = rest.find(')')?;
                return Some(format!("fn {}({})", name, &rest[..close]));
            }
        }
    }
    None
}

fn get_signature_help(uri: &str, line: usize, character: usize) -> serde_json::Value {
    let Some(text) = get_document(uri).or_else(|| read_document(uri)) else {
        return serde_json::Value::Null;
    };
    let Some(line_text) = text.lines().nth(line) else {
        return serde_json::Value::Null;
    };
    let Some((name, active)) = find_enclosing_call(line_text, character) else {
        return serde_json::Value::Null;
    };

    let (label, documentation) = if let Some(doc) = builtin_doc(&name) {
        let (sig, desc) = doc.split_once(" — ").unwrap_or((doc, ""));
        (sig.to_string(), desc.to_string())
    } else if let Some(sig) = user_fn_signature(&text, &name) {
        (sig, String::new())
    } else {
        return serde_json::Value::Null;
    };

    let params = signature_params(&label);
    let active = if params.is_empty() {
        0
    } else {
        active.min(params.len() - 1)
    };
    serde_json::json!({
        "signatures": [{
            "label": label,
            "documentation": documentation,
            "parameters": params.iter().map(|p| serde_json::json!({"label": p})).collect::<Vec<_>>()
        }],
        "activeSignature": 0,
        "activeParameter": active
    })
}

fn get_diagnostics(source: &str) -> Vec<serde_json::Value> {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            return vec![serde_json::json!({
                "range": {
                    "start": {"line": e.line.saturating_sub(1), "character": e.col.saturating_sub(1)},
                    "end": {"line": e.line.saturating_sub(1), "character": e.col}
                },
                "severity": 1,
                "message": e.message
            })];
        }
    };

    let mut parser = crate::parser::Parser::new(tokens);
    match parser.parse_program() {
        Ok(program) => {
            let source_lines: Vec<&str> = source.lines().collect();
            let mut checker = crate::typechecker::TypeChecker::with_strict(false);
            let warnings = checker.check(&program);
            warnings
                .into_iter()
                .map(|w| {
                    let line = w.line.saturating_sub(1);
                    let end_char = source_lines.get(line).map(|l| l.len()).unwrap_or(0);
                    let severity = if w.is_error { 1 } else { 2 };
                    serde_json::json!({
                        "range": {
                            "start": {"line": line, "character": 0},
                            "end": {"line": line, "character": end_char}
                        },
                        "severity": severity,
                        "source": "forge-typecheck",
                        "message": w.message
                    })
                })
                .collect()
        }
        Err(e) => {
            vec![serde_json::json!({
                "range": {
                    "start": {"line": e.line.saturating_sub(1), "character": e.col.saturating_sub(1)},
                    "end": {"line": e.line.saturating_sub(1), "character": e.col}
                },
                "severity": 1,
                "message": e.message
            })]
        }
    }
}

fn get_completions() -> Vec<serde_json::Value> {
    let keywords = [
        "let",
        "mut",
        "fn",
        "define",
        "return",
        "if",
        "else",
        "otherwise",
        "nah",
        "match",
        "for",
        "each",
        "in",
        "while",
        "loop",
        "break",
        "continue",
        "set",
        "to",
        "change",
        "say",
        "yell",
        "whisper",
        "grab",
        "from",
        "wait",
        "seconds",
        "repeat",
        "times",
        "try",
        "catch",
        "type",
        "struct",
        "interface",
        "import",
        "spawn",
        "true",
        "false",
        "forge",
        "hold",
        "emit",
        "unpack",
        "assert",
        "assert_eq",
    ];
    let builtins = [
        "println",
        "print",
        "say",
        "yell",
        "whisper",
        "len",
        "type",
        "str",
        "int",
        "float",
        "push",
        "pop",
        "map",
        "filter",
        "reduce",
        "sort",
        "reverse",
        "keys",
        "values",
        "contains",
        "range",
        "enumerate",
        "split",
        "join",
        "replace",
        "starts_with",
        "ends_with",
        "Ok",
        "Err",
        "Some",
        "None",
        "is_ok",
        "is_err",
        "is_some",
        "is_none",
        "unwrap",
        "unwrap_or",
        "assert",
        "assert_eq",
        "fetch",
        "time",
        "uuid",
        "wait",
        "exit",
        "run_command",
    ];
    let modules = [
        "math", "fs", "io", "crypto", "db", "pg", "mysql", "env", "json", "regex", "log", "http",
        "csv", "term", "time", "jwt", "npc", "exec",
    ];

    let mut items = Vec::new();
    for kw in &keywords {
        items.push(serde_json::json!({"label": kw, "kind": 14})); // Keyword
    }
    for bi in &builtins {
        items.push(serde_json::json!({"label": bi, "kind": 3})); // Function
    }
    for m in &modules {
        items.push(serde_json::json!({"label": m, "kind": 9})); // Module
    }
    items
}

fn get_module_completions(params: &serde_json::Value) -> Vec<serde_json::Value> {
    // Detect which module the user typed before the dot
    let typed_module = detect_module_prefix(params);

    let module_members: std::collections::HashMap<&str, Vec<&str>> = [
        (
            "math",
            vec![
                "sqrt",
                "pow",
                "abs",
                "max",
                "min",
                "floor",
                "ceil",
                "round",
                "random",
                "random_int",
                "sin",
                "cos",
                "tan",
                "log",
                "pi",
                "e",
                "clamp",
            ],
        ),
        (
            "fs",
            vec![
                "read",
                "write",
                "append",
                "exists",
                "list",
                "remove",
                "mkdir",
                "copy",
                "rename",
                "size",
                "ext",
                "read_json",
                "write_json",
                "lines",
                "dirname",
                "basename",
                "join_path",
                "is_dir",
                "is_file",
                "temp_dir",
            ],
        ),
        (
            "io",
            vec![
                "prompt",
                "print",
                "args",
                "args_parse",
                "args_get",
                "args_has",
            ],
        ),
        (
            "crypto",
            vec![
                "sha256",
                "md5",
                "base64_encode",
                "base64_decode",
                "hex_encode",
                "hex_decode",
            ],
        ),
        (
            "db",
            vec!["open", "query", "execute", "close", "last_insert_rowid"],
        ),
        ("pg", vec!["connect", "query", "execute", "close"]),
        ("mysql", vec!["connect", "query", "execute", "close"]),
        ("jwt", vec!["sign", "verify", "decode", "valid"]),
        ("env", vec!["get", "set", "keys", "has"]),
        ("json", vec!["parse", "stringify", "pretty"]),
        (
            "regex",
            vec!["test", "find", "find_all", "replace", "split"],
        ),
        ("log", vec!["info", "warn", "error", "debug"]),
        (
            "http",
            vec![
                "get", "post", "put", "delete", "patch", "head", "download", "crawl",
            ],
        ),
        ("csv", vec!["parse", "stringify", "read", "write"]),
        (
            "term",
            vec![
                "red",
                "green",
                "blue",
                "yellow",
                "cyan",
                "magenta",
                "bold",
                "dim",
                "table",
                "hr",
                "clear",
                "confirm",
                "sparkline",
                "bar",
                "banner",
                "box",
                "gradient",
                "success",
                "error",
            ],
        ),
        (
            "time",
            vec![
                "now",
                "unix",
                "parse",
                "format",
                "diff",
                "add",
                "sub",
                "zone",
                "zones",
                "elapsed",
                "today",
                "date",
                "sleep",
                "measure",
                "local",
                "is_before",
                "is_after",
                "start_of",
                "end_of",
                "from_unix",
                "is_weekend",
                "is_weekday",
                "day_of_week",
                "days_in_month",
                "is_leap_year",
            ],
        ),
        (
            "npc",
            vec![
                "name",
                "first_name",
                "last_name",
                "email",
                "username",
                "phone",
                "number",
                "pick",
                "bool",
                "sentence",
                "word",
                "id",
                "color",
                "ip",
                "url",
                "company",
            ],
        ),
        ("exec", vec!["run_command"]),
    ]
    .into_iter()
    .collect();

    let mut items = Vec::new();

    // If we detected a specific module prefix, only return that module's members
    if let Some(ref prefix) = typed_module {
        if let Some(members) = module_members.get(prefix.as_str()) {
            for member in members {
                items.push(serde_json::json!({
                    "label": member,
                    "kind": 3,
                    "detail": format!("{}.{}", prefix, member),
                }));
            }
            return items;
        }
    }

    // Fallback: return all module members (shouldn't normally happen)
    for (module, members) in &module_members {
        for member in members {
            items.push(serde_json::json!({
                "label": member,
                "kind": 3,
                "detail": format!("{}.{}", module, member),
                "sortText": format!("0_{}", member),
            }));
        }
    }
    items
}

/// Detect which module name the user typed before the `.` trigger character.
/// Reads the document text and cursor position to extract the identifier before the dot.
fn detect_module_prefix(params: &serde_json::Value) -> Option<String> {
    let uri = params.pointer("/textDocument/uri")?.as_str()?;
    let line = params.pointer("/position/line")?.as_u64()? as usize;
    let character = params.pointer("/position/character")?.as_u64()? as usize;

    let text = get_document(uri).or_else(|| read_document(uri))?;
    let line_text = text.lines().nth(line)?;

    // The dot is at `character`, so the module name ends just before it
    if character == 0 {
        return None;
    }
    let chars: Vec<char> = line_text.chars().collect();
    let dot_pos = character.min(chars.len());
    if dot_pos == 0 {
        return None;
    }

    // Walk backwards from just before the dot to find the identifier
    let mut end = dot_pos;
    // Skip the dot itself if cursor is on it
    if end > 0 && chars.get(end.wrapping_sub(1)) == Some(&'.') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    if start == end {
        return None;
    }

    let word: String = chars[start..end].iter().collect();
    Some(word)
}

#[derive(Debug, Clone)]
struct DocumentSymbolInfo {
    name: String,
    kind: u64,
    line: usize,
}

fn collect_document_symbols(source: &str) -> Vec<DocumentSymbolInfo> {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(_) => return Vec::new(),
    };
    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(program) => program,
        Err(_) => return Vec::new(),
    };

    let mut symbols = Vec::new();
    for spanned in program.statements {
        let (name, kind) = match spanned.stmt {
            Stmt::FnDef { name, .. } => (name, 12),
            Stmt::Let { name, .. } => (name, 13),
            Stmt::StructDef { name, .. } => (name, 23),
            Stmt::TypeDef { name, .. } => (name, 10),
            Stmt::InterfaceDef { name, .. } => (name, 11),
            Stmt::PromptDef { name, .. } => (name, 12),
            Stmt::AgentDef { name, .. } => (name, 5),
            _ => continue,
        };
        symbols.push(DocumentSymbolInfo {
            name,
            kind,
            line: spanned.line.saturating_sub(1),
        });
    }
    symbols
}

fn symbol_range(source: &str, line: usize, name: &str) -> serde_json::Value {
    let line_text = source.lines().nth(line).unwrap_or("");
    let start = line_text.find(name).unwrap_or(0);
    serde_json::json!({
        "start": { "line": line, "character": start },
        "end": { "line": line, "character": start + name.len() }
    })
}

fn get_document_symbols(uri: &str) -> Vec<serde_json::Value> {
    let Some(text) = get_document(uri).or_else(|| read_document(uri)) else {
        return Vec::new();
    };

    collect_document_symbols(&text)
        .into_iter()
        .map(|symbol| {
            let range = symbol_range(&text, symbol.line, &symbol.name);
            serde_json::json!({
                "name": symbol.name,
                "kind": symbol.kind,
                "range": range.clone(),
                "selectionRange": range
            })
        })
        .collect()
}

fn get_definition(uri: &str, line: usize, character: usize) -> serde_json::Value {
    let Some(text) = get_document(uri).or_else(|| read_document(uri)) else {
        return serde_json::Value::Null;
    };
    let line_text = text.lines().nth(line).unwrap_or("");
    let word = extract_word_at(line_text, character);
    if word.is_empty() {
        return serde_json::Value::Null;
    }

    // Try deep symbol search first (includes params, locals inside functions)
    let deep = collect_all_symbols(&text);
    if let Some(symbol) = deep.iter().find(|s| s.name == word) {
        return serde_json::json!({
            "uri": uri,
            "range": symbol_range(&text, symbol.line, &symbol.name)
        });
    }

    // Cross-file: check imported files for the symbol
    let imports = collect_imports(&text);
    for (import_path, names) in &imports {
        // If named import, only follow if the word is in the name list
        if let Some(name_list) = names {
            if !name_list.iter().any(|n| n == &word) {
                continue;
            }
        }
        if let Some(resolved) = resolve_import_for_lsp(import_path, uri) {
            if let Ok(imported_text) = std::fs::read_to_string(&resolved) {
                let exported = collect_exported_symbols(&imported_text);
                if let Some(symbol) = exported.iter().find(|s| s.name == word) {
                    let target_uri = path_to_uri(&resolved);
                    return serde_json::json!({
                        "uri": target_uri,
                        "range": symbol_range(&imported_text, symbol.line, &symbol.name)
                    });
                }
            }
        }
    }

    serde_json::Value::Null
}

/// Find word-boundary references in a single text, returning results tagged with the given URI.
fn find_word_references(text: &str, word: &str, target_uri: &str) -> Vec<serde_json::Value> {
    let is_ident_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut results = Vec::new();
    for (line_num, line_content) in text.lines().enumerate() {
        let mut search_from = 0;
        let bytes = line_content.as_bytes();
        while let Some(col) = line_content[search_from..].find(word) {
            let abs_col = search_from + col;
            let after_pos = abs_col + word.len();
            let before_ok = abs_col == 0 || !is_ident_byte(bytes[abs_col - 1]);
            let after_ok = after_pos >= bytes.len() || !is_ident_byte(bytes[after_pos]);

            if before_ok && after_ok {
                results.push(serde_json::json!({
                    "uri": target_uri,
                    "range": {
                        "start": { "line": line_num, "character": abs_col },
                        "end": { "line": line_num, "character": abs_col + word.len() }
                    }
                }));
            }
            search_from = abs_col + word.len();
        }
    }
    results
}

/// Find all references to a symbol across the current file and imported files.
fn get_references(uri: &str, line: usize, character: usize) -> Vec<serde_json::Value> {
    let Some(text) = get_document(uri).or_else(|| read_document(uri)) else {
        return Vec::new();
    };
    let line_text = text.lines().nth(line).unwrap_or("");
    let word = extract_word_at(line_text, character);
    if word.is_empty() {
        return Vec::new();
    }

    // Search current file
    let mut results = find_word_references(&text, &word, uri);

    // Cross-file: search imported files
    let imports = collect_imports(&text);
    let mut searched = std::collections::HashSet::new();
    searched.insert(uri.to_string());

    for (import_path, _) in &imports {
        if let Some(resolved) = resolve_import_for_lsp(import_path, uri) {
            let imported_uri = path_to_uri(&resolved);
            if searched.contains(&imported_uri) {
                continue;
            }
            searched.insert(imported_uri.clone());
            if let Ok(imported_text) = std::fs::read_to_string(&resolved) {
                results.extend(find_word_references(&imported_text, &word, &imported_uri));
            }
        }
    }

    // Also search files that import the current file (reverse references)
    // Look for .fg files in the same directory
    if let Some(current_path) = uri.strip_prefix("file://") {
        if let Some(parent) = std::path::Path::new(current_path).parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "fg") {
                        let sibling_uri = path_to_uri(&path);
                        if searched.contains(&sibling_uri) {
                            continue;
                        }
                        searched.insert(sibling_uri.clone());
                        if let Ok(sibling_text) = std::fs::read_to_string(&path) {
                            // Only include if this file imports or references the word
                            let refs = find_word_references(&sibling_text, &word, &sibling_uri);
                            if !refs.is_empty() {
                                results.extend(refs);
                            }
                        }
                    }
                }
            }
        }
    }

    results
}

/// Collect all symbols including those inside function bodies (params, locals, nested fns).
fn collect_all_symbols(source: &str) -> Vec<DocumentSymbolInfo> {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(_) => return Vec::new(),
    };
    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(program) => program,
        Err(_) => return Vec::new(),
    };

    let mut symbols = Vec::new();
    for spanned in &program.statements {
        collect_symbols_from_stmt(&spanned.stmt, spanned.line.saturating_sub(1), &mut symbols);
    }
    symbols
}

fn collect_symbols_from_stmt(stmt: &Stmt, line: usize, symbols: &mut Vec<DocumentSymbolInfo>) {
    match stmt {
        Stmt::FnDef {
            name, params, body, ..
        } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 12,
                line,
            });
            // Add parameters as variable symbols
            for param in params {
                symbols.push(DocumentSymbolInfo {
                    name: param.name.clone(),
                    kind: 13,
                    line,
                });
            }
            // Recurse into body
            for inner in body {
                collect_symbols_from_stmt(&inner.stmt, inner.line.saturating_sub(1), symbols);
            }
        }
        Stmt::Let { name, .. } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 13,
                line,
            });
        }
        Stmt::StructDef { name, .. } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 23,
                line,
            });
        }
        Stmt::TypeDef { name, .. } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 10,
                line,
            });
        }
        Stmt::InterfaceDef { name, .. } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 11,
                line,
            });
        }
        Stmt::PromptDef { name, .. } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 12,
                line,
            });
        }
        Stmt::AgentDef { name, .. } => {
            symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 5,
                line,
            });
        }
        Stmt::For {
            var, var2, body, ..
        } => {
            symbols.push(DocumentSymbolInfo {
                name: var.clone(),
                kind: 13,
                line,
            });
            if let Some(v2) = var2 {
                symbols.push(DocumentSymbolInfo {
                    name: v2.clone(),
                    kind: 13,
                    line,
                });
            }
            for s in body {
                collect_symbols_from_stmt(&s.stmt, s.line.saturating_sub(1), symbols);
            }
        }
        Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        } => {
            for s in try_body {
                collect_symbols_from_stmt(&s.stmt, s.line.saturating_sub(1), symbols);
            }
            symbols.push(DocumentSymbolInfo {
                name: catch_var.clone(),
                kind: 13,
                line,
            });
            for s in catch_body {
                collect_symbols_from_stmt(&s.stmt, s.line.saturating_sub(1), symbols);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_symbols_from_stmt(&s.stmt, s.line.saturating_sub(1), symbols);
            }
            if let Some(eb) = else_body {
                for s in eb {
                    collect_symbols_from_stmt(&s.stmt, s.line.saturating_sub(1), symbols);
                }
            }
        }
        Stmt::ImplBlock { methods, .. } => {
            for m in methods {
                collect_symbols_from_stmt(&m.stmt, m.line.saturating_sub(1), symbols);
            }
        }
        Stmt::While { body, .. }
        | Stmt::Loop { body, .. }
        | Stmt::Spawn { body }
        | Stmt::SafeBlock { body }
        | Stmt::TimeoutBlock { body, .. }
        | Stmt::RetryBlock { body, .. }
        | Stmt::ScheduleBlock { body, .. }
        | Stmt::WatchBlock { body, .. } => {
            for s in body {
                collect_symbols_from_stmt(&s.stmt, s.line.saturating_sub(1), symbols);
            }
        }
        Stmt::Import { path, names } => {
            // Show named imports as symbols; for wildcard imports, show the module path
            if let Some(name_list) = names {
                for n in name_list {
                    symbols.push(DocumentSymbolInfo {
                        name: n.clone(),
                        kind: 2, // Module
                        line,
                    });
                }
            } else {
                symbols.push(DocumentSymbolInfo {
                    name: path.clone(),
                    kind: 2,
                    line,
                });
            }
        }
        _ => {}
    }
}

/// Look up the one-line doc (`fn name(params) -> T — description`) for a
/// builtin function or module.
fn builtin_doc(name: &str) -> Option<&'static str> {
    BUILTIN_DOCS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, doc)| *doc)
}

const BUILTIN_DOCS: &[(&str, &str)] = &[
    (
        "println",
        "fn println(...args) — Print values followed by a newline",
    ),
    (
        "print",
        "fn print(...args) — Print values without a newline",
    ),
    ("say", "fn say(...args) — Print with natural language style"),
    ("yell", "fn yell(...args) — Print in UPPERCASE"),
    ("whisper", "fn whisper(...args) — Print in lowercase"),
    (
        "len",
        "fn len(value) -> Int — Get the length of a string, array, or object",
    ),
    (
        "type",
        "fn type(value) -> String — Get the type name of a value",
    ),
    ("typeof", "fn typeof(value) -> String — Alias for type()"),
    ("str", "fn str(value) -> String — Convert a value to string"),
    ("int", "fn int(value) -> Int — Convert a value to integer"),
    (
        "float",
        "fn float(value) -> Float — Convert a value to float",
    ),
    (
        "push",
        "fn push(array, value) — Add an element to the end of an array",
    ),
    (
        "pop",
        "fn pop(array) -> Value — Remove and return the last element",
    ),
    ("map", "fn map(array, fn) -> Array — Transform each element"),
    (
        "filter",
        "fn filter(array, fn) -> Array — Keep elements matching predicate",
    ),
    (
        "reduce",
        "fn reduce(array, fn, init) -> Value — Fold array to single value",
    ),
    (
        "sort",
        "fn sort(array) -> Array — Sort array in ascending order",
    ),
    (
        "reverse",
        "fn reverse(array) -> Array — Reverse array order",
    ),
    (
        "keys",
        "fn keys(object) -> Array — Get all keys of an object",
    ),
    (
        "values",
        "fn values(object) -> Array — Get all values of an object",
    ),
    (
        "contains",
        "fn contains(collection, value) -> Bool — Check if collection contains value",
    ),
    (
        "range",
        "fn range(start, end) -> Array — Generate integer range [start, end)",
    ),
    (
        "enumerate",
        "fn enumerate(array) -> Array — Pairs of [index, value]",
    ),
    (
        "split",
        "fn split(string, delimiter) -> Array — Split string into parts",
    ),
    (
        "join",
        "fn join(array, separator) -> String — Join array elements into string",
    ),
    (
        "replace",
        "fn replace(string, from, to) -> String — Replace occurrences in string",
    ),
    (
        "starts_with",
        "fn starts_with(string, prefix) -> Bool — Check string prefix",
    ),
    (
        "ends_with",
        "fn ends_with(string, suffix) -> Bool — Check string suffix",
    ),
    (
        "fetch",
        "fn fetch(url) -> Object — HTTP GET request, returns {status, body, headers}",
    ),
    ("uuid", "fn uuid() -> String — Generate a random UUID v4"),
    (
        "assert",
        "fn assert(condition) — Panic if condition is false",
    ),
    ("assert_eq", "fn assert_eq(a, b) — Panic if a != b"),
    ("assert_ne", "fn assert_ne(a, b) — Panic if a == b"),
    (
        "assert_throws",
        "fn assert_throws(fn) — Assert that function throws an error",
    ),
    (
        "Ok",
        "fn Ok(value) -> Result — Wrap value in a success Result",
    ),
    (
        "Err",
        "fn Err(message) -> Result — Wrap message in an error Result",
    ),
    ("is_ok", "fn is_ok(result) -> Bool — Check if Result is Ok"),
    (
        "is_err",
        "fn is_err(result) -> Bool — Check if Result is Err",
    ),
    (
        "unwrap",
        "fn unwrap(result) -> Value — Extract value from Ok, panic on Err",
    ),
    (
        "unwrap_or",
        "fn unwrap_or(result, default) -> Value — Extract value or use default",
    ),
    ("Some", "fn Some(value) -> Option — Wrap value in Some"),
    ("None", "None — The absence of a value"),
    (
        "is_some",
        "fn is_some(option) -> Bool — Check if Option has a value",
    ),
    (
        "is_none",
        "fn is_none(option) -> Bool — Check if Option is None",
    ),
    (
        "sh",
        "fn sh(command) -> String — Run shell command, return stdout",
    ),
    (
        "exit",
        "fn exit(code) — Exit the program with a status code",
    ),
    (
        "input",
        "fn input(prompt) -> String — Read a line from stdin",
    ),
    (
        "time",
        "fn time() -> Int — Current unix timestamp in seconds",
    ),
    (
        "sum",
        "fn sum(array) -> Number — Sum all elements in an array",
    ),
    (
        "min_of",
        "fn min_of(array) -> Value — Find minimum value in array",
    ),
    (
        "max_of",
        "fn max_of(array) -> Value — Find maximum value in array",
    ),
    ("unique", "fn unique(array) -> Array — Remove duplicates"),
    (
        "flatten",
        "fn flatten(array) -> Array — Flatten nested arrays",
    ),
    (
        "zip",
        "fn zip(a, b) -> Array — Combine two arrays into pairs",
    ),
    (
        "chunk",
        "fn chunk(array, size) -> Array — Split array into chunks",
    ),
    (
        "find",
        "fn find(array, fn) -> Value — Find first matching element",
    ),
    (
        "any",
        "fn any(array, fn) -> Bool — Check if any element matches",
    ),
    (
        "all",
        "fn all(array, fn) -> Bool — Check if all elements match",
    ),
    (
        "has_key",
        "fn has_key(object, key) -> Bool — Check if object has key",
    ),
    (
        "merge",
        "fn merge(obj1, obj2) -> Object — Merge two objects",
    ),
    (
        "pick",
        "fn pick(object, keys) -> Object — Select specific keys",
    ),
    (
        "omit",
        "fn omit(object, keys) -> Object — Exclude specific keys",
    ),
    (
        "entries",
        "fn entries(object) -> Array — Get [key, value] pairs",
    ),
    (
        "from_entries",
        "fn from_entries(array) -> Object — Create object from pairs",
    ),
    // Module docs
    (
        "math",
        "module math — Math functions: sqrt, pow, abs, sin, cos, random_int, pi, e, ...",
    ),
    (
        "fs",
        "module fs — File system: read, write, append, exists, list, remove, mkdir, ...",
    ),
    (
        "io",
        "module io — Input/output: prompt, print, args, args_parse, args_get, args_has",
    ),
    (
        "crypto",
        "module crypto — Cryptography: sha256, md5, base64_encode/decode, hex_encode/decode",
    ),
    (
        "db",
        "module db — SQLite database: open, query, execute, close, last_insert_rowid",
    ),
    (
        "pg",
        "module pg — PostgreSQL: connect, query, execute, close",
    ),
    (
        "mysql",
        "module mysql — MySQL: connect, query, execute, close",
    ),
    (
        "jwt",
        "module jwt — JSON Web Tokens: sign, verify, decode, valid",
    ),
    (
        "env",
        "module env — Environment variables: get, set, has, keys",
    ),
    ("json", "module json — JSON: parse, stringify, pretty"),
    (
        "regex",
        "module regex — Regular expressions: test, find, find_all, replace, split",
    ),
    ("log", "module log — Logging: info, warn, error, debug"),
    (
        "http",
        "module http — HTTP client: get, post, put, delete, patch, head, download, crawl",
    ),
    ("csv", "module csv — CSV: parse, stringify, read, write"),
    (
        "term",
        "module term — Terminal: red, green, blue, bold, table, hr, sparkline, bar, banner, box",
    ),
    (
        "npc",
        "module npc — Fake data: name, email, username, phone, number, pick, bool, sentence, ...",
    ),
    ("exec", "module exec — Shell execution: run_command"),
    // GenZ debug kit
    (
        "sus",
        "fn sus(value) — Inspect a value (GenZ debug: equivalent to dbg!)",
    ),
    (
        "bruh",
        "fn bruh(message) — Panic with a message (GenZ debug)",
    ),
    (
        "bet",
        "fn bet(condition) — Assert condition is true (GenZ debug)",
    ),
    ("no_cap", "fn no_cap(a, b) — Assert equality (GenZ debug)"),
    (
        "ick",
        "fn ick(condition) — Assert condition is false (GenZ debug)",
    ),
    ("yolo", "fn yolo(fn) — Fire-and-forget execution"),
    ("cook", "fn cook(fn) — Profile execution time"),
    ("slay", "fn slay(fn, iterations) — Benchmark a function"),
    (
        "ghost",
        "fn ghost(fn) — Silent execution (suppresses output)",
    ),
];

fn get_hover(uri: &str, line: usize, character: usize) -> serde_json::Value {
    let doc_text = get_document(uri).or_else(|| read_document(uri));
    if let Some(text) = doc_text {
        let lines: Vec<&str> = text.lines().collect();
        if let Some(line_text) = lines.get(line) {
            let word = extract_word_at(line_text, character);

            // Check builtins first
            if let Some(doc) = builtin_doc(&word) {
                return serde_json::json!({
                    "contents": {
                        "kind": "markdown",
                        "value": format!("```forge\n{}\n```", doc)
                    }
                });
            }

            // Check user-defined symbols
            if let Some(hover_text) = get_user_symbol_hover(&text, &word) {
                return serde_json::json!({
                    "contents": {
                        "kind": "markdown",
                        "value": hover_text
                    }
                });
            }
        }
    }

    serde_json::Value::Null
}

/// Generate hover text for a user-defined symbol (function, variable, struct, etc.)
fn get_user_symbol_hover(source: &str, name: &str) -> Option<String> {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize().ok()?;
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser.parse_program().ok()?;

    for spanned in &program.statements {
        if let Some(hover) = hover_from_stmt(&spanned.stmt, name) {
            return Some(hover);
        }
    }
    None
}

fn hover_from_stmt(stmt: &Stmt, name: &str) -> Option<String> {
    match stmt {
        Stmt::FnDef {
            name: fn_name,
            params,
            return_type,
            is_async,
            body,
            ..
        } => {
            if fn_name == name {
                let params_str = params
                    .iter()
                    .map(|p| {
                        let mut s = p.name.clone();
                        if let Some(ref t) = p.type_ann {
                            s.push_str(&format!(": {}", format_type_ann(t)));
                        }
                        if let Some(ref d) = p.default {
                            s.push_str(&format!(" = {}", format_expr_brief(d)));
                        }
                        s
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = return_type
                    .as_ref()
                    .map(|t| format!(" -> {}", format_type_ann(t)))
                    .unwrap_or_default();
                let prefix = if *is_async { "async fn" } else { "fn" };
                return Some(format!(
                    "```forge\n{} {}({}){}\n```",
                    prefix, fn_name, params_str, ret
                ));
            }
            // Search inside function body
            for inner in body {
                if let Some(hover) = hover_from_stmt(&inner.stmt, name) {
                    return Some(hover);
                }
            }
            None
        }
        Stmt::Let {
            name: var_name,
            mutable,
            type_ann,
            ..
        } => {
            if var_name == name {
                let mut_str = if *mutable { "let mut" } else { "let" };
                let type_str = type_ann
                    .as_ref()
                    .map(|t| format!(": {}", format_type_ann(t)))
                    .unwrap_or_default();
                return Some(format!(
                    "```forge\n{} {}{}\n```",
                    mut_str, var_name, type_str
                ));
            }
            None
        }
        Stmt::StructDef {
            name: struct_name,
            fields,
            ..
        } => {
            if struct_name == name {
                let fields_str = fields
                    .iter()
                    .map(|f| format!("  {}: {}", f.name, format_type_ann(&f.type_ann)))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(format!(
                    "```forge\nthing {} {{\n{}\n}}\n```",
                    struct_name, fields_str
                ));
            }
            None
        }
        Stmt::TypeDef {
            name: type_name,
            variants,
        } => {
            if type_name == name {
                let variants_str = variants
                    .iter()
                    .map(|v| v.name.clone())
                    .collect::<Vec<_>>()
                    .join(" | ");
                return Some(format!(
                    "```forge\ntype {} = {}\n```",
                    type_name, variants_str
                ));
            }
            None
        }
        Stmt::InterfaceDef {
            name: iface_name,
            methods,
        } => {
            if iface_name == name {
                let methods_str = methods
                    .iter()
                    .map(|m| {
                        let params = m
                            .params
                            .iter()
                            .map(|p| p.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("  fn {}({})", m.name, params)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(format!(
                    "```forge\ninterface {} {{\n{}\n}}\n```",
                    iface_name, methods_str
                ));
            }
            None
        }
        Stmt::ImplBlock { methods, .. } => {
            for m in methods {
                if let Some(h) = hover_from_stmt(&m.stmt, name) {
                    return Some(h);
                }
            }
            None
        }
        // Recurse into blocks
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                if let Some(h) = hover_from_stmt(&s.stmt, name) {
                    return Some(h);
                }
            }
            if let Some(eb) = else_body {
                for s in eb {
                    if let Some(h) = hover_from_stmt(&s.stmt, name) {
                        return Some(h);
                    }
                }
            }
            None
        }
        Stmt::For { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Loop { body, .. }
        | Stmt::Spawn { body }
        | Stmt::SafeBlock { body }
        | Stmt::TimeoutBlock { body, .. }
        | Stmt::RetryBlock { body, .. }
        | Stmt::ScheduleBlock { body, .. }
        | Stmt::WatchBlock { body, .. } => {
            for s in body {
                if let Some(h) = hover_from_stmt(&s.stmt, name) {
                    return Some(h);
                }
            }
            None
        }
        Stmt::TryCatch {
            try_body,
            catch_body,
            ..
        } => {
            for s in try_body {
                if let Some(h) = hover_from_stmt(&s.stmt, name) {
                    return Some(h);
                }
            }
            for s in catch_body {
                if let Some(h) = hover_from_stmt(&s.stmt, name) {
                    return Some(h);
                }
            }
            None
        }
        _ => None,
    }
}

fn format_type_ann(t: &crate::parser::ast::TypeAnn) -> String {
    use crate::parser::ast::TypeAnn;
    match t {
        TypeAnn::Simple(s) => s.clone(),
        TypeAnn::Array(inner) => format!("[{}]", format_type_ann(inner)),
        TypeAnn::Generic(name, args) => {
            let args_str = args
                .iter()
                .map(format_type_ann)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", name, args_str)
        }
        TypeAnn::Function(params, ret) => {
            let params_str = params
                .iter()
                .map(format_type_ann)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) -> {}", params_str, format_type_ann(ret))
        }
        TypeAnn::Optional(inner) => format!("{}?", format_type_ann(inner)),
        TypeAnn::Tuple(items) => {
            let inner = items
                .iter()
                .map(format_type_ann)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
    }
}

fn format_expr_brief(expr: &crate::parser::ast::Expr) -> String {
    use crate::parser::ast::Expr;
    match expr {
        Expr::Int(i) => i.to_string(),
        Expr::Float(f) => f.to_string(),
        Expr::StringLit(s) => format!("\"{}\"", s),
        Expr::Bool(b) => b.to_string(),
        _ => "...".to_string(),
    }
}

/// Extract the word at a given character position in a line.
fn extract_word_at(line: &str, character: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if character >= chars.len() {
        return String::new();
    }

    let is_ident = |c: char| c.is_alphanumeric() || c == '_';

    let mut start = character;
    while start > 0 && is_ident(chars[start - 1]) {
        start -= 1;
    }

    let mut end = character;
    while end < chars.len() && is_ident(chars[end]) {
        end += 1;
    }

    chars[start..end].iter().collect()
}

/// Read a document from a file:// URI.
fn read_document(uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://")?;
    std::fs::read_to_string(path).ok()
}

fn path_to_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

/// Extract import statements from source code.
fn collect_imports(source: &str) -> Vec<(String, Option<Vec<String>>)> {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let mut imports = Vec::new();
    for spanned in &program.statements {
        if let Stmt::Import { path, names } = &spanned.stmt {
            imports.push((path.clone(), names.clone()));
        }
    }
    imports
}

/// Resolve an import path to a file path, given the importing file's URI.
fn resolve_import_for_lsp(import_path: &str, current_uri: &str) -> Option<std::path::PathBuf> {
    let current_file = current_uri.strip_prefix("file://")?;
    let base_dir = std::path::Path::new(current_file).parent();
    crate::package::resolve_import_from(import_path, base_dir)
}

/// Collect top-level (exported) symbols from a file — only FnDef, Let, StructDef,
/// TypeDef, InterfaceDef at the top level (not nested inside functions).
fn collect_exported_symbols(source: &str) -> Vec<DocumentSymbolInfo> {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let mut symbols = Vec::new();
    for spanned in &program.statements {
        let line = spanned.line.saturating_sub(1);
        match &spanned.stmt {
            Stmt::FnDef { name, .. } => symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 12,
                line,
            }),
            Stmt::Let { name, .. } => symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 13,
                line,
            }),
            Stmt::StructDef { name, .. } => symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 23,
                line,
            }),
            Stmt::TypeDef { name, .. } => symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 10,
                line,
            }),
            Stmt::InterfaceDef { name, .. } => symbols.push(DocumentSymbolInfo {
                name: name.clone(),
                kind: 11,
                line,
            }),
            _ => {}
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_navigation_capabilities() {
        let response =
            handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
                .unwrap();
        let json: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(
            json.pointer("/result/capabilities/definitionProvider")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            json.pointer("/result/capabilities/documentSymbolProvider")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn collect_document_symbols_finds_top_level_items() {
        let symbols = collect_document_symbols(
            r#"
            fn add(a, b) { return a + b }
            let answer = add(20, 22)
            thing User { name: String }
            "#,
        );

        assert!(symbols
            .iter()
            .any(|symbol| symbol.name == "add" && symbol.kind == 12));
        assert!(symbols
            .iter()
            .any(|symbol| symbol.name == "answer" && symbol.kind == 13));
        assert!(symbols
            .iter()
            .any(|symbol| symbol.name == "User" && symbol.kind == 23));
    }

    #[test]
    fn unknown_request_returns_method_not_found_error() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":42,"method":"textDocument/codeAction","params":{}}"#,
        )
        .expect("requests must always get a response");
        let json: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(
            json.pointer("/error/code").and_then(|v| v.as_i64()),
            Some(-32601)
        );
        assert_eq!(
            json.pointer("/id").and_then(|v| v.as_i64()),
            Some(42),
            "the response must echo the request id"
        );
        let msg = json
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(msg.contains("textDocument/codeAction"));
    }

    #[test]
    fn unknown_notification_is_silently_ignored() {
        // No id field — this is a notification, not a request, so dropping it
        // is the spec-compliant behaviour.
        let response =
            handle_message(r#"{"jsonrpc":"2.0","method":"$/some/notification","params":{}}"#);
        assert!(response.is_none());
    }

    #[test]
    fn definition_returns_matching_symbol_location() {
        let uri = "file:///tmp/forge-lsp-definition.fg";
        let text = "fn add(a, b) { return a + b }\nlet answer = add(20, 22)\nanswer\n";
        store_document(uri, text);

        let definition = get_definition(uri, 1, 14);
        assert_eq!(
            definition
                .pointer("/range/start/line")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            definition
                .pointer("/range/start/character")
                .and_then(|value| value.as_u64()),
            Some(3)
        );
    }

    #[test]
    fn hover_shows_user_defined_function_signature() {
        let uri = "file:///tmp/forge-lsp-hover-fn.fg";
        let text = "fn greet(name: String, age: Int) -> String { return name }\ngreet(\"hi\", 1)\n";
        store_document(uri, text);

        let hover = get_hover(uri, 1, 0);
        let value = hover
            .pointer("/contents/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(value.contains("fn greet(name: String, age: Int) -> String"));
    }

    #[test]
    fn hover_shows_user_defined_variable() {
        let uri = "file:///tmp/forge-lsp-hover-var.fg";
        let text = "let mut count = 0\ncount\n";
        store_document(uri, text);

        let hover = get_hover(uri, 1, 0);
        let value = hover
            .pointer("/contents/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(value.contains("let mut count"));
    }

    #[test]
    fn hover_shows_struct_fields() {
        let uri = "file:///tmp/forge-lsp-hover-struct.fg";
        let text = "thing User { name: String, age: Int }\nUser\n";
        store_document(uri, text);

        let hover = get_hover(uri, 1, 0);
        let value = hover
            .pointer("/contents/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(value.contains("thing User"));
        assert!(value.contains("name: String"));
    }

    #[test]
    fn hover_returns_null_for_unknown_symbol() {
        let uri = "file:///tmp/forge-lsp-hover-unknown.fg";
        let text = "let x = 1\nunknown_thing\n";
        store_document(uri, text);

        let hover = get_hover(uri, 1, 0);
        assert!(hover.is_null());
    }

    #[test]
    fn references_finds_all_occurrences() {
        let uri = "file:///tmp/forge-lsp-refs.fg";
        let text = "let count = 0\ncount = count + 1\nsay(count)\n";
        store_document(uri, text);

        let refs = get_references(uri, 0, 4);
        assert!(
            refs.len() >= 3,
            "expected at least 3 references to 'count', got {}",
            refs.len()
        );
    }

    #[test]
    fn references_respects_word_boundaries() {
        let uri = "file:///tmp/forge-lsp-refs-boundary.fg";
        let text = "let name = \"test\"\nlet name_long = \"other\"\nname\n";
        store_document(uri, text);

        let refs = get_references(uri, 0, 4);
        // Should find "name" on lines 0 and 2, but NOT inside "name_long"
        assert_eq!(
            refs.len(),
            2,
            "expected 2 references to 'name', got {}",
            refs.len()
        );
    }

    #[test]
    fn deep_symbols_finds_function_params() {
        let symbols = collect_all_symbols("fn add(a, b) { let result = a + b }\n");
        assert!(symbols.iter().any(|s| s.name == "add"));
        assert!(symbols.iter().any(|s| s.name == "a"));
        assert!(symbols.iter().any(|s| s.name == "b"));
        assert!(symbols.iter().any(|s| s.name == "result"));
    }

    #[test]
    fn initialize_advertises_references_capability() {
        let response =
            handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
                .unwrap();
        let json: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(
            json.pointer("/result/capabilities/referencesProvider")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn module_completions_context_aware() {
        let uri = "file:///tmp/forge-lsp-module-ctx.fg";
        let text = "let x = math.sqrt(4)\n";
        store_document(uri, text);

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 13 },
            "context": { "triggerCharacter": "." }
        });
        let completions = get_module_completions(&params);
        // All completions should be from math module
        for item in &completions {
            let detail = item.get("detail").and_then(|d| d.as_str()).unwrap_or("");
            assert!(
                detail.starts_with("math."),
                "expected math module, got: {}",
                detail
            );
        }
        assert!(!completions.is_empty());
    }

    #[test]
    fn diagnostics_reports_type_warnings() {
        let diags = get_diagnostics("let x: Int = \"hello\"");
        assert_eq!(diags.len(), 1);
        let msg = diags[0]
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(msg.contains("type mismatch"));
        // Severity 2 = warning (not strict mode)
        assert_eq!(diags[0].get("severity").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(
            diags[0].get("source").and_then(|v| v.as_str()),
            Some("forge-typecheck")
        );
    }

    #[test]
    fn diagnostics_includes_line_number() {
        let diags = get_diagnostics("let y = 1\nlet x: Int = \"hello\"");
        assert_eq!(diags.len(), 1);
        // Line 2 in source (1-indexed) → line 1 in LSP (0-indexed)
        let line = diags[0]
            .pointer("/range/start/line")
            .and_then(|v| v.as_u64());
        assert_eq!(line, Some(1));
    }

    #[test]
    fn diagnostics_empty_for_valid_code() {
        let diags = get_diagnostics("let x = 42\nlet y = x + 1");
        assert!(diags.is_empty());
    }

    #[test]
    fn diagnostics_reports_arity_mismatch() {
        let diags = get_diagnostics("fn add(a, b) { return a + b }\nadd(1, 2, 3)");
        assert_eq!(diags.len(), 1);
        assert!(diags[0]
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("expects 2"));
    }

    #[test]
    fn collect_imports_extracts_import_statements() {
        let imports = collect_imports("import \"helper\"\nimport { foo, bar } from \"utils\"");
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].0, "helper");
        assert!(imports[0].1.is_none());
        assert_eq!(imports[1].0, "utils");
        assert_eq!(
            imports[1].1.as_ref().unwrap(),
            &vec!["foo".to_string(), "bar".to_string()]
        );
    }

    #[test]
    fn collect_exported_symbols_finds_top_level_only() {
        let symbols = collect_exported_symbols(
            "fn greet(name) {\n  let msg = \"hi\"\n  return msg\n}\nlet x = 42\n",
        );
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"x"));
        // Inner variable 'msg' should NOT be exported
        assert!(!names.contains(&"msg"));
    }

    #[test]
    fn cross_file_definition_resolves_import() {
        // Create temp files
        let dir = std::env::temp_dir().join("forge-lsp-test-xfile");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Write a helper module
        std::fs::write(
            dir.join("helper.fg"),
            "fn greet(name) {\n  return \"hi \" + name\n}\n",
        )
        .unwrap();

        // Write a main file that imports helper
        let main_source = "import \"helper\"\nlet result = greet(\"world\")\n";
        std::fs::write(dir.join("main.fg"), main_source).unwrap();

        let main_uri = format!("file://{}", dir.join("main.fg").display());
        store_document(&main_uri, main_source);

        // Ask for definition of "greet" on line 1, char 13
        let def = get_definition(&main_uri, 1, 13);
        assert!(!def.is_null(), "should find cross-file definition");
        let def_uri = def.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            def_uri.contains("helper.fg"),
            "definition should point to helper.fg, got: {}",
            def_uri
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cross_file_references_finds_across_files() {
        let dir = std::env::temp_dir().join("forge-lsp-test-xref");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("lib.fg"), "fn helper() { return 1 }\n").unwrap();
        let main_source = "import \"lib\"\nlet x = helper()\n";
        std::fs::write(dir.join("main.fg"), main_source).unwrap();

        let main_uri = format!("file://{}", dir.join("main.fg").display());
        store_document(&main_uri, main_source);

        // Find references to "helper" from main.fg line 1, char 8
        let refs = get_references(&main_uri, 1, 8);
        // Should find at least 2: one in main.fg (usage), one in lib.fg (definition)
        assert!(
            refs.len() >= 2,
            "should find references across files, found: {}",
            refs.len()
        );

        let uris: Vec<&str> = refs
            .iter()
            .filter_map(|r| r.get("uri").and_then(|v| v.as_str()))
            .collect();
        assert!(uris.iter().any(|u| u.contains("main.fg")));
        assert!(uris.iter().any(|u| u.contains("lib.fg")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_symbols_show_in_document_symbols() {
        let symbols = collect_all_symbols("import { foo, bar } from \"utils\"\nlet x = 1\n");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"x"));
    }

    // ---- serve loop (in-memory transport) ----

    fn recv(client: &Connection) -> Message {
        client
            .receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("server did not respond within 10s")
    }

    fn request(id: i32, method: &str, params: serde_json::Value) -> Message {
        Request::new(id.into(), method.to_string(), params).into()
    }

    fn note(method: &str, params: serde_json::Value) -> Message {
        Notification::new(method.to_string(), params).into()
    }

    #[test]
    fn serve_handles_full_session_and_clean_exit() {
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || serve(&server));

        client
            .sender
            .send(request(
                1,
                "initialize",
                serde_json::json!({"capabilities": {}}),
            ))
            .unwrap();
        let Message::Response(init) = recv(&client) else {
            panic!("expected initialize response")
        };
        let caps = init.response_result.unwrap();
        assert_eq!(
            caps.pointer("/capabilities/documentFormattingProvider"),
            Some(&serde_json::json!(true))
        );
        client
            .sender
            .send(note("initialized", serde_json::json!({})))
            .unwrap();

        let uri = "file:///tmp/forge-lsp-serve-test.fg";
        client
            .sender
            .send(note(
                "textDocument/didOpen",
                serde_json::json!({"textDocument": {"uri": uri, "languageId": "forge", "version": 1, "text": "let x = (\n"}}),
            ))
            .unwrap();
        let Message::Notification(diag) = recv(&client) else {
            panic!("expected publishDiagnostics")
        };
        assert_eq!(diag.method, "textDocument/publishDiagnostics");
        assert!(!diag.params["diagnostics"].as_array().unwrap().is_empty());

        // Unknown request -> MethodNotFound, and the server keeps going.
        client
            .sender
            .send(request(2, "textDocument/codeAction", serde_json::json!({})))
            .unwrap();
        let Message::Response(resp) = recv(&client) else {
            panic!("expected response")
        };
        assert_eq!(
            resp.response_result.unwrap_err().code,
            ErrorCode::MethodNotFound as i32
        );

        client
            .sender
            .send(request(3, "shutdown", serde_json::Value::Null))
            .unwrap();
        let Message::Response(resp) = recv(&client) else {
            panic!("expected shutdown response")
        };
        assert_eq!(resp.id, 3.into());
        client
            .sender
            .send(note("exit", serde_json::Value::Null))
            .unwrap();
        assert!(
            handle.join().unwrap().unwrap(),
            "shutdown + exit is a clean exit"
        );
    }

    #[test]
    fn serve_reports_unclean_exit_without_shutdown() {
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || serve(&server));
        client
            .sender
            .send(request(
                1,
                "initialize",
                serde_json::json!({"capabilities": {}}),
            ))
            .unwrap();
        recv(&client);
        client
            .sender
            .send(note("initialized", serde_json::json!({})))
            .unwrap();
        client
            .sender
            .send(note("exit", serde_json::Value::Null))
            .unwrap();
        assert!(!handle.join().unwrap().unwrap());
    }

    #[test]
    fn invalid_params_return_error_not_silence() {
        let response =
            handle_message(r#"{"jsonrpc":"2.0","id":7,"method":"textDocument/hover","params":{}}"#)
                .expect("requests must always get a response");
        let json: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(
            json.pointer("/error/code").and_then(|v| v.as_i64()),
            Some(-32602)
        );
    }

    #[test]
    fn did_close_clears_diagnostics() {
        let uri = "file:///tmp/forge-lsp-close.fg";
        store_document(uri, "let x = 1\n");
        let out = handle_notification(&Notification::new(
            "textDocument/didClose".to_string(),
            serde_json::json!({"textDocument": {"uri": uri}}),
        ));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].params["diagnostics"], serde_json::json!([]));
        assert!(get_document(uri).is_none());
    }

    #[test]
    fn formatting_returns_whole_document_edit() {
        let uri = "file:///tmp/forge-lsp-format.fg";
        store_document(uri, "fn f() {\nsay 1\n}");
        let edits = get_formatting_edits(uri);
        let edits = edits.as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["newText"], "fn f() {\n    say 1\n}\n");
        assert_eq!(
            edits[0]["range"]["end"],
            serde_json::json!({"line": 2, "character": 1})
        );

        store_document(uri, "fn f() {\n    say 1\n}\n");
        assert_eq!(get_formatting_edits(uri), serde_json::json!([]));
    }

    #[test]
    fn signature_help_for_builtin_and_user_fn() {
        let uri = "file:///tmp/forge-lsp-sig.fg";
        store_document(
            uri,
            "fn add(a, b) { return a + b }\nlet r = add(1, \nlet s = replace(\"x\", \"y\", \n",
        );
        let help = get_signature_help(uri, 1, 15);
        assert_eq!(help["signatures"][0]["label"], "fn add(a, b)");
        assert_eq!(help["activeParameter"], 1);

        let help = get_signature_help(uri, 2, 26);
        assert!(help["signatures"][0]["label"]
            .as_str()
            .unwrap()
            .starts_with("fn replace(string, from, to)"));
        assert_eq!(help["activeParameter"], 2);
        assert_eq!(
            help["signatures"][0]["parameters"]
                .as_array()
                .unwrap()
                .len(),
            3
        );

        // Not inside a call.
        assert!(get_signature_help(uri, 0, 3).is_null());
    }

    #[test]
    fn find_enclosing_call_skips_nested_and_strings() {
        assert_eq!(
            find_enclosing_call("foo(a, bar(1, 2), ", 18),
            Some(("foo".to_string(), 2))
        );
        assert_eq!(
            find_enclosing_call("foo(\"a,(b\", ", 12),
            Some(("foo".to_string(), 1))
        );
        assert_eq!(find_enclosing_call("foo(1)", 6), None);
        assert_eq!(find_enclosing_call("let a = [1, ", 12), None);
    }
}
