#![allow(dead_code)]

use std::io::{self, BufRead, Write};

/// Basic LSP server for Forge.
/// Implements the Language Server Protocol over stdin/stdout.
/// Provides: diagnostics (parse errors), completions (keywords/builtins).

pub fn run_lsp() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    eprintln!("Forge LSP server started");

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.starts_with("Content-Length:") {
            let len: usize = line
                .trim_start_matches("Content-Length:")
                .trim()
                .parse()
                .unwrap_or(0);

            // Read empty line
            let mut empty = String::new();
            io::stdin().read_line(&mut empty).ok();

            // Read content
            let mut content = vec![0u8; len];
            io::stdin().lock().read_exact(&mut content).ok();
            let body = String::from_utf8_lossy(&content).to_string();

            if let Some(response) = handle_message(&body) {
                let resp_bytes = response.as_bytes();
                write!(
                    stdout,
                    "Content-Length: {}\r\n\r\n{}",
                    resp_bytes.len(),
                    response
                )
                .ok();
                stdout.flush().ok();
            }
        }
    }
}

fn handle_message(body: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    let method = json.get("method")?.as_str()?;
    let id = json.get("id");

    match method {
        "initialize" => {
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "capabilities": {
                        "textDocumentSync": 1,
                        "completionProvider": {
                            "triggerCharacters": ["."]
                        },
                        "hoverProvider": true
                    }
                }
            });
            Some(result.to_string())
        }
        "initialized" => None,
        "textDocument/didOpen" | "textDocument/didChange" => {
            let params = json.get("params")?;
            let doc = params.get("textDocument")?;
            let uri = doc.get("uri")?.as_str()?;
            let text = if method == "textDocument/didOpen" {
                doc.get("text")?.as_str()?
            } else {
                let changes = params.get("contentChanges")?.as_array()?;
                changes.first()?.get("text")?.as_str()?
            };

            let diagnostics = get_diagnostics(text);
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {
                    "uri": uri,
                    "diagnostics": diagnostics
                }
            });
            Some(notification.to_string())
        }
        "textDocument/completion" => {
            let params = json.get("params")?;
            let context = params
                .pointer("/context/triggerCharacter")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let completions = if context == "." {
                get_module_completions(params)
            } else {
                get_completions()
            };
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": completions
            });
            Some(result.to_string())
        }
        "textDocument/hover" => {
            let params = json.get("params")?;
            let doc = params.get("textDocument")?;
            let uri = doc.get("uri")?.as_str()?;
            let position = params.get("position")?;
            let line = position.get("line")?.as_u64()? as usize;
            let character = position.get("character")?.as_u64()? as usize;
            let hover = get_hover(uri, line, character);
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": hover
            });
            Some(result.to_string())
        }
        "shutdown" => {
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": null
            });
            Some(result.to_string())
        }
        _ => None,
    }
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
        Ok(_) => Vec::new(),
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
        "math", "fs", "io", "crypto", "db", "pg", "env", "json", "regex", "log", "http", "csv",
        "term", "time",
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
    let _doc = params.get("textDocument");
    let module_members: std::collections::HashMap<&str, Vec<&str>> = [
        (
            "math",
            vec![
                "sqrt", "pow", "abs", "max", "min", "floor", "ceil", "round", "random", "sin",
                "cos", "tan", "log", "pi", "e",
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
            ],
        ),
        ("io", vec!["prompt", "print", "args"]),
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
        ("db", vec!["open", "query", "execute", "close"]),
        ("pg", vec!["connect", "query", "execute", "close"]),
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
                "red", "green", "blue", "yellow", "cyan", "magenta", "bold", "dim", "table", "hr",
                "clear", "confirm",
            ],
        ),
        (
            "time",
            vec![
                "now", "unix", "parse", "format", "diff", "add", "sub", "zone", "elapsed", "today",
                "sleep", "measure", "local",
            ],
        ),
    ]
    .into_iter()
    .collect();

    let mut items = Vec::new();
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

fn get_hover(_uri: &str, _line: usize, _character: usize) -> serde_json::Value {
    let builtins: std::collections::HashMap<&str, &str> = [
        (
            "println",
            "fn println(...args) — Print values followed by a newline",
        ),
        (
            "print",
            "fn print(...args) — Print values without a newline",
        ),
        ("say", "fn say(...args) — Print with natural language style"),
        (
            "len",
            "fn len(value) -> Int — Get the length of a string, array, or object",
        ),
        (
            "type",
            "fn type(value) -> String — Get the type name of a value",
        ),
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
            "range",
            "fn range(start, end) -> Array — Generate integer range [start, end)",
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
        (
            "Ok",
            "fn Ok(value) -> Result — Wrap value in a success Result",
        ),
        (
            "Err",
            "fn Err(message) -> Result — Wrap message in an error Result",
        ),
        (
            "unwrap",
            "fn unwrap(result) -> Value — Extract value from Ok, panic on Err",
        ),
        (
            "sh",
            "fn sh(command) -> String — Run shell command, return stdout",
        ),
    ]
    .into_iter()
    .collect();

    // Without document text context, return generic hover
    serde_json::json!({
        "contents": {
            "kind": "markdown",
            "value": "Forge Language Server"
        }
    })
}

use std::io::Read;
