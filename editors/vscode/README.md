# Forge Language — VS Code Extension

Language support for the [Forge programming language](https://github.com/humancto/forge-lang): a language server client, a debugger, syntax highlighting and snippets.

## Features

- **Language server** (`forge lsp`): diagnostics for parse errors and type-check warnings, hover, completion (including `module.` members), go to definition, find references, document symbols / outline, **Format Document** (same output as `forge fmt`) and signature help.
- **Debugger** (`forge dap`): breakpoints, step over / into / out, call stack, variables, stop on entry. Press F5 in an open `.fg` file to debug it without writing a `launch.json`.
- **Syntax highlighting** for Forge keywords, builtins, modules and operators.
- **24 code snippets** for common patterns (functions, loops, HTTP servers, …).
- **Language configuration**: bracket matching, auto-closing pairs, indentation rules.

## Requirements

The extension runs the `forge` binary. Install Forge and make sure `forge` is on your `PATH`, or point the `forge.path` setting at it:

```bash
forge version   # should print the Forge version
forge lsp       # language server over stdio (used by the extension)
forge dap       # debug adapter over stdio (used by the extension)
```

## Settings

| Setting              | Default   | Description                                                                  |
| -------------------- | --------- | ---------------------------------------------------------------------------- |
| `forge.path`         | `"forge"` | Path to the `forge` executable used for `forge lsp` and `forge dap`.         |
| `forge.lsp.enabled`  | `true`    | Start the language server.                                                   |
| `forge.trace.server` | `"off"`   | Log LSP traffic (`messages` / `verbose`) to the **Forge** output channel.    |

Changing `forge.path` or `forge.lsp.enabled` restarts the server. You can also run **Forge: Restart Language Server** from the command palette.

## Debugging

F5 on a `.fg` file debugs it directly. To customise, add a configuration to `.vscode/launch.json`:

```json
{
  "type": "forge",
  "request": "launch",
  "name": "Debug Forge program",
  "program": "${file}",
  "stopOnEntry": false
}
```

The debugger uses Forge's tree-walking interpreter.

## Installation

### From a `.vsix`

```bash
cd editors/vscode
npm install
npm run package                       # runs `vsce package`
code --install-extension forge-lang-0.3.0.vsix
```

### From source (development)

```bash
cd editors/vscode
npm install                           # installs vscode-languageclient
ln -s "$PWD" ~/.vscode/extensions/forge-lang
```

Then reload VS Code (**Developer: Reload Window**). The extension is plain JavaScript (`extension.js`), so there is no build step. `npm run check` syntax-checks it.

### Publishing

```bash
npm run package     # produce forge-lang-<version>.vsix
npm run publish     # requires a Marketplace PAT for the `forge-lang` publisher (vsce login forge-lang)
```

Bump `version` in `package.json` and add a `CHANGELOG.md` entry first.

## Snippets

Type the prefix and press `Tab`:

| Prefix         | Description                |
| -------------- | -------------------------- |
| `fn`           | Function definition        |
| `define`       | Function (natural syntax)  |
| `if` / `ife`   | If / if-else block         |
| `for`          | For-in loop                |
| `repeat`       | Repeat N times             |
| `match`        | Match expression           |
| `when`         | When guard                 |
| `let` / `letm` | Variable / mutable binding |
| `set`          | Variable (natural syntax)  |
| `say`          | Print output               |
| `struct`       | Struct definition          |
| `import`       | Import statement           |
| `retry`        | Retry block                |
| `safe`         | Safe execution block       |
| `try`          | Try-catch block            |
| `server`       | HTTP server scaffolding    |
| `httpget`      | HTTP GET request           |
| `grab`         | Fetch URL (natural syntax) |
| `test`         | Test function              |
| `schedule`     | Scheduled task             |
| `check`        | Declarative validation     |

## Other editors

Any LSP-capable editor can use the server: configure it to run `forge lsp` (stdio) for `.fg` files. Debug Adapter Protocol clients can launch `forge dap` with a `launch` request containing `program` and optional `stopOnEntry`.
