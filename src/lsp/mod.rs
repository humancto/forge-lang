#![allow(dead_code)]

use crate::parser::ast::Stmt;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
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

/// Basic LSP server for Forge.
/// Implements the Language Server Protocol over stdin/stdout.
/// Provides: diagnostics (parse errors), completions, hover, definition, document symbols.

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
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "referencesProvider": true,
                        "documentSymbolProvider": true
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

            store_document(uri, text);

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
        "textDocument/definition" => {
            let params = json.get("params")?;
            let doc = params.get("textDocument")?;
            let uri = doc.get("uri")?.as_str()?;
            let position = params.get("position")?;
            let line = position.get("line")?.as_u64()? as usize;
            let character = position.get("character")?.as_u64()? as usize;
            let definition = get_definition(uri, line, character);
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": definition
            });
            Some(result.to_string())
        }
        "textDocument/references" => {
            let params = json.get("params")?;
            let doc = params.get("textDocument")?;
            let uri = doc.get("uri")?.as_str()?;
            let position = params.get("position")?;
            let line = position.get("line")?.as_u64()? as usize;
            let character = position.get("character")?.as_u64()? as usize;
            let references = get_references(uri, line, character);
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": references
            });
            Some(result.to_string())
        }
        "textDocument/documentSymbol" => {
            let params = json.get("params")?;
            let doc = params.get("textDocument")?;
            let uri = doc.get("uri")?.as_str()?;
            let symbols = get_document_symbols(uri);
            let result = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": symbols
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
        // Per LSP spec, requests (messages with an id) for unhandled methods
        // must return a JSON-RPC MethodNotFound error rather than silently
        // dropping the request. Notifications (no id) may still be ignored.
        other => {
            if id.is_some() {
                let error = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("method not found: {}", other)
                    }
                });
                Some(error.to_string())
            } else {
                None
            }
        }
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
        Ok(program) => {
            let mut checker = crate::typechecker::TypeChecker::with_strict(false);
            let warnings = checker.check(&program);
            warnings
                .into_iter()
                .map(|w| {
                    let line = w.line.saturating_sub(1);
                    let severity = if w.is_error { 1 } else { 2 };
                    serde_json::json!({
                        "range": {
                            "start": {"line": line, "character": 0},
                            "end": {"line": line, "character": 0}
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

    serde_json::Value::Null
}

/// Find all references to a symbol in the document (text-based search with word boundaries).
fn get_references(uri: &str, line: usize, character: usize) -> Vec<serde_json::Value> {
    let Some(text) = get_document(uri).or_else(|| read_document(uri)) else {
        return Vec::new();
    };
    let line_text = text.lines().nth(line).unwrap_or("");
    let word = extract_word_at(line_text, character);
    if word.is_empty() {
        return Vec::new();
    }

    let is_ident_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    let mut results = Vec::new();
    for (line_num, line_content) in text.lines().enumerate() {
        let mut search_from = 0;
        let bytes = line_content.as_bytes();
        while let Some(col) = line_content[search_from..].find(&word) {
            let abs_col = search_from + col;
            let after_pos = abs_col + word.len();
            let before_ok = abs_col == 0 || !is_ident_byte(bytes[abs_col - 1]);
            let after_ok = after_pos >= bytes.len() || !is_ident_byte(bytes[after_pos]);

            if before_ok && after_ok {
                results.push(serde_json::json!({
                    "uri": uri,
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
                collect_symbols_from_stmt(inner, line, symbols);
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
                collect_symbols_from_stmt(s, line, symbols);
            }
        }
        Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        } => {
            for s in try_body {
                collect_symbols_from_stmt(s, line, symbols);
            }
            symbols.push(DocumentSymbolInfo {
                name: catch_var.clone(),
                kind: 13,
                line,
            });
            for s in catch_body {
                collect_symbols_from_stmt(s, line, symbols);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_symbols_from_stmt(s, line, symbols);
            }
            if let Some(eb) = else_body {
                for s in eb {
                    collect_symbols_from_stmt(s, line, symbols);
                }
            }
        }
        Stmt::ImplBlock { methods, .. } => {
            for m in methods {
                collect_symbols_from_stmt(m, line, symbols);
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
                collect_symbols_from_stmt(s, line, symbols);
            }
        }
        _ => {}
    }
}

fn get_hover(uri: &str, line: usize, character: usize) -> serde_json::Value {
    let builtins: std::collections::HashMap<&str, &str> = [
        ("println", "fn println(...args) — Print values followed by a newline"),
        ("print", "fn print(...args) — Print values without a newline"),
        ("say", "fn say(...args) — Print with natural language style"),
        ("yell", "fn yell(...args) — Print in UPPERCASE"),
        ("whisper", "fn whisper(...args) — Print in lowercase"),
        ("len", "fn len(value) -> Int — Get the length of a string, array, or object"),
        ("type", "fn type(value) -> String — Get the type name of a value"),
        ("typeof", "fn typeof(value) -> String — Alias for type()"),
        ("str", "fn str(value) -> String — Convert a value to string"),
        ("int", "fn int(value) -> Int — Convert a value to integer"),
        ("float", "fn float(value) -> Float — Convert a value to float"),
        ("push", "fn push(array, value) — Add an element to the end of an array"),
        ("pop", "fn pop(array) -> Value — Remove and return the last element"),
        ("map", "fn map(array, fn) -> Array — Transform each element"),
        ("filter", "fn filter(array, fn) -> Array — Keep elements matching predicate"),
        ("reduce", "fn reduce(array, fn, init) -> Value — Fold array to single value"),
        ("sort", "fn sort(array) -> Array — Sort array in ascending order"),
        ("reverse", "fn reverse(array) -> Array — Reverse array order"),
        ("keys", "fn keys(object) -> Array — Get all keys of an object"),
        ("values", "fn values(object) -> Array — Get all values of an object"),
        ("contains", "fn contains(collection, value) -> Bool — Check if collection contains value"),
        ("range", "fn range(start, end) -> Array — Generate integer range [start, end)"),
        ("enumerate", "fn enumerate(array) -> Array — Pairs of [index, value]"),
        ("split", "fn split(string, delimiter) -> Array — Split string into parts"),
        ("join", "fn join(array, separator) -> String — Join array elements into string"),
        ("replace", "fn replace(string, from, to) -> String — Replace occurrences in string"),
        ("starts_with", "fn starts_with(string, prefix) -> Bool — Check string prefix"),
        ("ends_with", "fn ends_with(string, suffix) -> Bool — Check string suffix"),
        ("fetch", "fn fetch(url) -> Object — HTTP GET request, returns {status, body, headers}"),
        ("uuid", "fn uuid() -> String — Generate a random UUID v4"),
        ("assert", "fn assert(condition) — Panic if condition is false"),
        ("assert_eq", "fn assert_eq(a, b) — Panic if a != b"),
        ("assert_ne", "fn assert_ne(a, b) — Panic if a == b"),
        ("assert_throws", "fn assert_throws(fn) — Assert that function throws an error"),
        ("Ok", "fn Ok(value) -> Result — Wrap value in a success Result"),
        ("Err", "fn Err(message) -> Result — Wrap message in an error Result"),
        ("is_ok", "fn is_ok(result) -> Bool — Check if Result is Ok"),
        ("is_err", "fn is_err(result) -> Bool — Check if Result is Err"),
        ("unwrap", "fn unwrap(result) -> Value — Extract value from Ok, panic on Err"),
        ("unwrap_or", "fn unwrap_or(result, default) -> Value — Extract value or use default"),
        ("Some", "fn Some(value) -> Option — Wrap value in Some"),
        ("None", "None — The absence of a value"),
        ("is_some", "fn is_some(option) -> Bool — Check if Option has a value"),
        ("is_none", "fn is_none(option) -> Bool — Check if Option is None"),
        ("sh", "fn sh(command) -> String — Run shell command, return stdout"),
        ("exit", "fn exit(code) — Exit the program with a status code"),
        ("input", "fn input(prompt) -> String — Read a line from stdin"),
        ("time", "fn time() -> Int — Current unix timestamp in seconds"),
        ("sum", "fn sum(array) -> Number — Sum all elements in an array"),
        ("min_of", "fn min_of(array) -> Value — Find minimum value in array"),
        ("max_of", "fn max_of(array) -> Value — Find maximum value in array"),
        ("unique", "fn unique(array) -> Array — Remove duplicates"),
        ("flatten", "fn flatten(array) -> Array — Flatten nested arrays"),
        ("zip", "fn zip(a, b) -> Array — Combine two arrays into pairs"),
        ("chunk", "fn chunk(array, size) -> Array — Split array into chunks"),
        ("find", "fn find(array, fn) -> Value — Find first matching element"),
        ("any", "fn any(array, fn) -> Bool — Check if any element matches"),
        ("all", "fn all(array, fn) -> Bool — Check if all elements match"),
        ("has_key", "fn has_key(object, key) -> Bool — Check if object has key"),
        ("merge", "fn merge(obj1, obj2) -> Object — Merge two objects"),
        ("pick", "fn pick(object, keys) -> Object — Select specific keys"),
        ("omit", "fn omit(object, keys) -> Object — Exclude specific keys"),
        ("entries", "fn entries(object) -> Array — Get [key, value] pairs"),
        ("from_entries", "fn from_entries(array) -> Object — Create object from pairs"),
        // Module docs
        ("math", "module math — Math functions: sqrt, pow, abs, sin, cos, random_int, pi, e, ..."),
        ("fs", "module fs — File system: read, write, append, exists, list, remove, mkdir, ..."),
        ("io", "module io — Input/output: prompt, print, args, args_parse, args_get, args_has"),
        ("crypto", "module crypto — Cryptography: sha256, md5, base64_encode/decode, hex_encode/decode"),
        ("db", "module db — SQLite database: open, query, execute, close, last_insert_rowid"),
        ("pg", "module pg — PostgreSQL: connect, query, execute, close"),
        ("mysql", "module mysql — MySQL: connect, query, execute, close"),
        ("jwt", "module jwt — JSON Web Tokens: sign, verify, decode, valid"),
        ("env", "module env — Environment variables: get, set, has, keys"),
        ("json", "module json — JSON: parse, stringify, pretty"),
        ("regex", "module regex — Regular expressions: test, find, find_all, replace, split"),
        ("log", "module log — Logging: info, warn, error, debug"),
        ("http", "module http — HTTP client: get, post, put, delete, patch, head, download, crawl"),
        ("csv", "module csv — CSV: parse, stringify, read, write"),
        ("term", "module term — Terminal: red, green, blue, bold, table, hr, sparkline, bar, banner, box"),
        ("npc", "module npc — Fake data: name, email, username, phone, number, pick, bool, sentence, ..."),
        ("exec", "module exec — Shell execution: run_command"),
        // GenZ debug kit
        ("sus", "fn sus(value) — Inspect a value (GenZ debug: equivalent to dbg!)"),
        ("bruh", "fn bruh(message) — Panic with a message (GenZ debug)"),
        ("bet", "fn bet(condition) — Assert condition is true (GenZ debug)"),
        ("no_cap", "fn no_cap(a, b) — Assert equality (GenZ debug)"),
        ("ick", "fn ick(condition) — Assert condition is false (GenZ debug)"),
        ("yolo", "fn yolo(fn) — Fire-and-forget execution"),
        ("cook", "fn cook(fn) — Profile execution time"),
        ("slay", "fn slay(fn, iterations) — Benchmark a function"),
        ("ghost", "fn ghost(fn) — Silent execution (suppresses output)"),
    ]
    .into_iter()
    .collect();

    let doc_text = get_document(uri).or_else(|| read_document(uri));
    if let Some(text) = doc_text {
        let lines: Vec<&str> = text.lines().collect();
        if let Some(line_text) = lines.get(line) {
            let word = extract_word_at(line_text, character);

            // Check builtins first
            if let Some(doc) = builtins.get(word.as_str()) {
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
                if let Some(hover) = hover_from_stmt(inner, name) {
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
                if let Some(h) = hover_from_stmt(m, name) {
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
                if let Some(h) = hover_from_stmt(s, name) {
                    return Some(h);
                }
            }
            if let Some(eb) = else_body {
                for s in eb {
                    if let Some(h) = hover_from_stmt(s, name) {
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
                if let Some(h) = hover_from_stmt(s, name) {
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
                if let Some(h) = hover_from_stmt(s, name) {
                    return Some(h);
                }
            }
            for s in catch_body {
                if let Some(h) = hover_from_stmt(s, name) {
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

use std::io::Read;

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
}
