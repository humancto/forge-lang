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
                        }
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
            let completions = get_completions();
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": completions
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
        "math", "fs", "io", "crypto", "db", "env", "json", "regex", "log",
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

use std::io::Read;
