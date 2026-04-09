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
    let _doc = params.get("textDocument");
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

    let Some(symbol) = collect_document_symbols(&text)
        .into_iter()
        .find(|symbol| symbol.name == word)
    else {
        return serde_json::Value::Null;
    };

    serde_json::json!({
        "uri": uri,
        "range": symbol_range(&text, symbol.line, &symbol.name)
    })
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

    // Use in-memory document store (populated by didOpen/didChange),
    // falling back to disk for files not yet opened in the editor.
    let doc_text = get_document(uri).or_else(|| read_document(uri));
    if let Some(text) = doc_text {
        let lines: Vec<&str> = text.lines().collect();
        if let Some(line_text) = lines.get(line) {
            let word = extract_word_at(line_text, character);
            if let Some(doc) = builtins.get(word.as_str()) {
                return serde_json::json!({
                    "contents": {
                        "kind": "markdown",
                        "value": format!("```forge\n{}\n```", doc)
                    }
                });
            }
        }
    }

    serde_json::json!({
        "contents": {
            "kind": "markdown",
            "value": "Forge Language Server"
        }
    })
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
}
