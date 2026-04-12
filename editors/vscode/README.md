# Forge Language — VS Code Extension

Syntax highlighting and snippets for the [Forge programming language](https://github.com/humancto/forge-lang).

## Features

- **Syntax highlighting** for all Forge keywords, builtins, modules, and operators
- **24 code snippets** for common patterns (functions, loops, HTTP servers, etc.)
- **Language configuration** with bracket matching, auto-closing pairs, and indentation rules

## Installation

### From source (development)

```bash
# Symlink into your VS Code extensions directory
ln -s /path/to/forge-lang/editors/vscode ~/.vscode/extensions/forge-lang
```

Then reload VS Code (`Cmd+Shift+P` → "Reload Window").

### Package and install

```bash
# Requires vsce: npm install -g @vscode/vsce
cd editors/vscode
vsce package
code --install-extension forge-lang-0.2.0.vsix
```

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

## LSP Support

Forge includes a built-in language server. To use it, configure your editor to run `forge lsp` as the language server command for `.fg` files. LSP client integration for this extension is planned for a future release.
