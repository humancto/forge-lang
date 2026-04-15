# Plan: 7E.3 — VS Code extension improvements

## Goal

Enhance the existing VS Code extension at `editors/vscode/` with updated grammar, snippets, LSP client, and debugger launch config.

## Current state

- `package.json` — basic extension manifest (no LSP, no snippets, no debugger)
- `syntaxes/forge.tmLanguage.json` — TextMate grammar (missing newer keywords and modules)
- `language-configuration.json` — brackets, comments, indent rules (complete)

## Changes

### 1. Update TextMate grammar

- Add missing keywords: `must`, `safe`, `check`, `retry`, `timeout`, `schedule`, `watch`, `ask`, `download`, `crawl`, `when`, `give`, `freeze`, `metadata`
- Add missing modules to highlighting: `pg`, `mysql`, `http`, `csv`, `term`, `exec`, `time`, `npc`, `url`, `toml`, `ws`, `jwt`, `os`, `path`
- Add missing builtins: `sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok`, `which`, `cwd`, `cd`, `pipe_to`, `channel`, `send`, `receive`

### 2. Add snippets

Create `snippets/forge.json` with common patterns:

- `fn` → function definition
- `for` → for-in loop
- `if` → if block
- `match` → match block
- `server` → HTTP server scaffolding
- `test` → test function
- `retry` → retry block
- `say` → say statement

### 3. Add LSP client configuration

The Forge LSP is available via `forge lsp`. Add LSP client config to `package.json` so the extension auto-connects. This requires a minimal `extension.js` that spawns `forge lsp` as a language server.

Actually — VS Code extensions that only provide grammars/snippets/configs don't need any JS. Adding an LSP client requires `vscode-languageclient` dependency and a real extension entry point. This adds significant complexity (npm, node_modules, build step).

**Decision: skip LSP client for now.** The grammar + snippets + debugger config are the highest-value additions. LSP client can be a separate item.

### 4. Add debugger launch config

Add a `.vscode/launch.json` contribution in package.json for Forge DAP debugging. Actually — contributing debugger configs requires a debug adapter extension, which is even more complex than LSP.

**Decision: add a sample `.vscode/launch.json` file users can copy, not a full debugger extension.**

### 5. Add README for the extension

Brief README explaining how to install (symlink or copy to `~/.vscode/extensions/`).

## Files to touch

1. `editors/vscode/syntaxes/forge.tmLanguage.json` — update grammar
2. NEW: `editors/vscode/snippets/forge.json` — snippets
3. `editors/vscode/package.json` — add snippets contribution
4. NEW: `editors/vscode/README.md` — installation instructions

## Test strategy

- Open a .fg file in VS Code with the extension installed and verify highlighting
- Verify snippets trigger with tab completion
- No Rust tests needed — this is purely editor config

## Rollback

Revert the branch. All changes are in `editors/vscode/`.
