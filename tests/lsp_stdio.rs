//! End-to-end regression tests for `forge lsp` over real stdio pipes.
//!
//! Guards against the original deadlock where the server never answered the
//! first `initialize` request because it re-locked stdin while iterating it.

#[path = "support/stdio_rpc.rs"]
mod stdio_rpc;

use serde_json::{json, Value};
use stdio_rpc::StdioServer;

const URI: &str = "file:///tmp/forge-lsp-stdio-e2e.fg";

fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

fn response_for(server: &StdioServer, id: i64) -> Value {
    let resp = server.recv_until(&format!("response id {}", id), |m| m["id"] == json!(id));
    assert!(
        resp.get("error").is_none(),
        "request {} failed: {}",
        id,
        resp
    );
    resp["result"].clone()
}

fn diagnostics(server: &StdioServer) -> Vec<Value> {
    let note = server.recv_until("publishDiagnostics", |m| {
        m["method"] == "textDocument/publishDiagnostics"
    });
    assert_eq!(note["params"]["uri"], URI);
    note["params"]["diagnostics"].as_array().unwrap().clone()
}

fn initialize(server: &mut StdioServer) -> Value {
    server.send(request(
        1,
        "initialize",
        json!({"processId": null, "rootUri": null, "capabilities": {}}),
    ));
    let result = response_for(server, 1);
    server.send(notification("initialized", json!({})));
    result
}

#[test]
fn lsp_full_session_over_stdio() {
    let mut server = StdioServer::spawn("lsp");

    let init = initialize(&mut server);
    let caps = &init["capabilities"];
    for cap in [
        "hoverProvider",
        "definitionProvider",
        "documentSymbolProvider",
        "documentFormattingProvider",
    ] {
        assert_eq!(caps[cap], json!(true), "missing capability {}", cap);
    }
    assert!(caps["completionProvider"].is_object());
    assert!(caps["signatureHelpProvider"].is_object());
    assert_eq!(init["serverInfo"]["name"], "forge-lsp");

    // didOpen with a parse error -> error diagnostic.
    server.send(notification(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": URI, "languageId": "forge", "version": 1,
               "text": "let x = (\n"}}),
    ));
    let diags = diagnostics(&server);
    assert!(!diags.is_empty(), "parse error must produce a diagnostic");
    assert_eq!(diags[0]["severity"], 1);

    // didChange to valid code -> diagnostics cleared.
    let text = "fn add(a, b) {\nreturn a + b\n}\nlet total = add(1, 2)\nprintln(total)\n";
    server.send(notification(
        "textDocument/didChange",
        json!({"textDocument": {"uri": URI, "version": 2},
               "contentChanges": [{"text": text}]}),
    ));
    assert!(diagnostics(&server).is_empty());

    let pos = |line: u32, character: u32| json!({"textDocument": {"uri": URI}, "position": {"line": line, "character": character}});

    // Hover on the call to `add`.
    server.send(request(2, "textDocument/hover", pos(3, 13)));
    let hover = response_for(&server, 2);
    let hover_text = hover["contents"]["value"].as_str().unwrap_or("");
    assert!(hover_text.contains("fn add(a, b)"), "hover: {}", hover);

    // Completion.
    server.send(request(3, "textDocument/completion", pos(4, 0)));
    let completion = response_for(&server, 3);
    let labels: Vec<&str> = completion
        .as_array()
        .expect("completion list")
        .iter()
        .filter_map(|c| c["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"println"),
        "completion labels: {:?}",
        labels
    );

    // Go to definition of `add` from its call site.
    server.send(request(4, "textDocument/definition", pos(3, 13)));
    let def = response_for(&server, 4);
    assert_eq!(def["range"]["start"]["line"], 0, "definition: {}", def);

    // Document symbols.
    server.send(request(
        5,
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": URI}}),
    ));
    let symbols = response_for(&server, 5);
    let names: Vec<&str> = symbols
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(
        names.contains(&"add") && names.contains(&"total"),
        "{:?}",
        names
    );

    // Formatting re-indents the function body.
    server.send(request(
        6,
        "textDocument/formatting",
        json!({"textDocument": {"uri": URI}, "options": {"tabSize": 4, "insertSpaces": true}}),
    ));
    let edits = response_for(&server, 6);
    let new_text = edits[0]["newText"].as_str().expect("one formatting edit");
    assert!(new_text.contains("\n    return a + b\n"), "{}", new_text);

    // Signature help inside `add(1, |`.
    server.send(request(7, "textDocument/signatureHelp", pos(3, 19)));
    let sig = response_for(&server, 7);
    assert_eq!(sig["signatures"][0]["label"], "fn add(a, b)");
    assert_eq!(sig["activeParameter"], 1);

    // Unknown request -> MethodNotFound error, not silence.
    server.send(request(8, "textDocument/codeLens", json!({})));
    let err = server.recv_until("response id 8", |m| m["id"] == json!(8));
    assert_eq!(err["error"]["code"], -32601);

    // Clean shutdown.
    server.send(request(9, "shutdown", Value::Null));
    assert_eq!(response_for(&server, 9), Value::Null);
    server.send(notification("exit", Value::Null));
    let status = server.wait_exit();
    assert!(
        status.success(),
        "exit after shutdown must be 0: {}",
        status
    );
}

#[test]
fn lsp_accepts_content_type_header_and_pipelined_messages() {
    let mut server = StdioServer::spawn("lsp");

    // initialize + initialized + a hover request in a single write, with an
    // optional Content-Type header on each message.
    let mut bytes = Vec::new();
    for msg in [
        request(1, "initialize", json!({"capabilities": {}})),
        notification("initialized", json!({})),
        request(
            2,
            "textDocument/hover",
            json!({"textDocument": {"uri": URI},
                 "position": {"line": 0, "character": 0}}),
        ),
    ] {
        let body = serde_json::to_vec(&msg).unwrap();
        bytes.extend_from_slice(
            format!(
                "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(&body);
    }
    server.write_raw(&bytes);

    let _ = response_for(&server, 1);
    // Unknown document -> hover is null, but it must still be answered.
    assert_eq!(response_for(&server, 2), Value::Null);
}

#[test]
fn lsp_exits_on_stdin_eof_without_hanging() {
    let mut server = StdioServer::spawn("lsp");
    initialize(&mut server);
    server.close_stdin();
    let status = server.wait_exit();
    // EOF without shutdown is an abnormal exit per the LSP spec.
    assert_eq!(status.code(), Some(1));
}

#[test]
fn lsp_exit_without_shutdown_is_nonzero() {
    let mut server = StdioServer::spawn("lsp");
    initialize(&mut server);
    server.send(notification("exit", Value::Null));
    let status = server.wait_exit();
    assert_eq!(status.code(), Some(1));
}

/// A two-file workspace: `lib.fg` defines `helper`, `main.fg` imports it.
fn workspace() -> (std::path::PathBuf, String, String) {
    let dir = std::env::temp_dir().join(format!("forge-lsp-ws-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    std::fs::write(
        dir.join("lib.fg"),
        "fn helper(x: Int) -> Int { return x * 2 }\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.fg"), MAIN).unwrap();
    let lib = format!("file://{}", dir.join("lib.fg").display());
    let main = format!("file://{}", dir.join("main.fg").display());
    (dir, lib, main)
}

const MAIN: &str = "import \"lib.fg\"\n\
let total = helper(21)\n\
fn shadow(total) { return total }\n\
say total + shadow(1)\n\
say totl\n";

fn at(uri: &str, line: u32, character: u32) -> Value {
    json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}})
}

#[test]
fn lsp_semantic_features_across_files() {
    let (dir, lib, main) = workspace();
    let mut server = StdioServer::spawn("lsp");
    server.send(request(
        1,
        "initialize",
        json!({"processId": null, "rootUri": format!("file://{}", dir.display()), "capabilities": {}}),
    ));
    let caps = response_for(&server, 1)["capabilities"].clone();
    assert_eq!(caps["renameProvider"]["prepareProvider"], true);
    assert_eq!(caps["codeActionProvider"], true);
    assert_eq!(caps["inlayHintProvider"], true);
    assert!(caps["semanticTokensProvider"]["legend"]["tokenTypes"].is_array());
    server.send(notification("initialized", json!({})));

    server.send(notification(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": main, "languageId": "forge", "version": 1, "text": MAIN}}),
    ));
    let note = server.recv_until("publishDiagnostics", |m| {
        m["method"] == "textDocument/publishDiagnostics"
    });
    let diags = note["params"]["diagnostics"].as_array().unwrap().clone();
    assert_eq!(diags.len(), 1, "{:?}", diags);
    assert_eq!(diags[0]["code"], "T0006");
    assert_eq!(
        diags[0]["range"]["start"],
        json!({"line": 4, "character": 4})
    );
    assert_eq!(diags[0]["range"]["end"], json!({"line": 4, "character": 8}));

    // Hover: inferred type of a variable, and an imported function.
    server.send(request(2, "textDocument/hover", at(&main, 1, 5)));
    let hover = response_for(&server, 2)["contents"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(hover.contains("let total: Int"), "{}", hover);
    server.send(request(3, "textDocument/hover", at(&main, 1, 14)));
    let hover = response_for(&server, 3)["contents"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(hover.contains("fn helper(x: Int) -> Int"), "{}", hover);
    assert!(hover.contains("lib.fg"), "{}", hover);

    // Go to definition follows the import into lib.fg.
    server.send(request(4, "textDocument/definition", at(&main, 1, 14)));
    let def = response_for(&server, 4);
    assert_eq!(def["uri"], lib);
    assert_eq!(def["range"]["start"], json!({"line": 0, "character": 3}));

    // References to `helper` from main span both files.
    server.send(request(
        5,
        "textDocument/references",
        json!({"textDocument": {"uri": main}, "position": {"line": 1, "character": 14},
               "context": {"includeDeclaration": true}}),
    ));
    let refs = response_for(&server, 5);
    let uris: Vec<&str> = refs
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .collect();
    assert!(
        uris.contains(&lib.as_str()) && uris.contains(&main.as_str()),
        "{:?}",
        refs
    );

    // Rename the top-level `total`: the shadowing parameter of `shadow`
    // (line 2) is a different variable and stays untouched.
    server.send(request(6, "textDocument/prepareRename", at(&main, 1, 5)));
    assert_eq!(response_for(&server, 6)["placeholder"], "total");
    server.send(request(
        7,
        "textDocument/rename",
        json!({"textDocument": {"uri": main}, "position": {"line": 1, "character": 5}, "newName": "sum_total"}),
    ));
    let edit = response_for(&server, 7);
    let edits = edit["changes"][&main].as_array().unwrap();
    let lines: Vec<u64> = edits
        .iter()
        .map(|e| e["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![1, 3], "{}", edit);

    // Renaming the imported function edits both files.
    server.send(request(
        8,
        "textDocument/rename",
        json!({"textDocument": {"uri": main}, "position": {"line": 1, "character": 14}, "newName": "twice"}),
    ));
    let edit = response_for(&server, 8);
    assert_eq!(
        edit["changes"][&lib].as_array().unwrap().len(),
        1,
        "{}",
        edit
    );
    assert_eq!(
        edit["changes"][&main].as_array().unwrap().len(),
        1,
        "{}",
        edit
    );

    // An invalid new name is refused.
    server.send(request(
        9,
        "textDocument/rename",
        json!({"textDocument": {"uri": main}, "position": {"line": 1, "character": 5}, "newName": "let"}),
    ));
    let err = server.recv_until("response id 9", |m| m["id"] == json!(9));
    assert!(err["error"].is_object(), "{}", err);

    // Quick fix for the typo.
    server.send(request(
        10,
        "textDocument/codeAction",
        json!({"textDocument": {"uri": main},
               "range": {"start": {"line": 4, "character": 5}, "end": {"line": 4, "character": 5}},
               "context": {"diagnostics": []}}),
    ));
    let actions = response_for(&server, 10);
    assert_eq!(actions[0]["title"], "Change to 'total'", "{}", actions);
    let fix = &actions[0]["edit"]["changes"][&main][0];
    assert_eq!(fix["newText"], "total");
    assert_eq!(fix["range"]["start"], json!({"line": 4, "character": 4}));

    // Inlay hint with the inferred type after `total`.
    server.send(request(
        11,
        "textDocument/inlayHint",
        json!({"textDocument": {"uri": main},
               "range": {"start": {"line": 0, "character": 0}, "end": {"line": 10, "character": 0}}}),
    ));
    let hints = response_for(&server, 11);
    assert_eq!(hints[0]["label"], ": Int", "{}", hints);
    assert_eq!(hints[0]["position"], json!({"line": 1, "character": 9}));

    // Semantic tokens: 5 integers per token, `helper` classified as a
    // function (index of "function" in the legend).
    server.send(request(
        12,
        "textDocument/semanticTokens/full",
        json!({"textDocument": {"uri": main}}),
    ));
    let data = response_for(&server, 12)["data"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!data.is_empty() && data.len() % 5 == 0);
    let legend: Vec<String> = caps["semanticTokensProvider"]["legend"]["tokenTypes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect();
    let function = legend.iter().position(|t| t == "function").unwrap() as u64;
    // Tokens are sorted: `total` (1:4) then `helper` (1:12).
    let second_type = data[8].as_u64().unwrap();
    assert_eq!(second_type, function, "{:?}", data);

    server.send(request(13, "shutdown", Value::Null));
    response_for(&server, 13);
    server.send(notification("exit", Value::Null));
    assert!(server.wait_exit().success());
    let _ = std::fs::remove_dir_all(&dir);
}
