# Contributing to Forge

Thank you for your interest in contributing to Forge! This guide will help you get set up and productive.

## Getting Started

### Prerequisites

- **Rust 1.85+** — install via [rustup](https://rustup.rs/)
- **Git**
- A code editor (VS Code recommended — we have syntax highlighting in `editors/vscode/`)

### Setup

```bash
git clone https://github.com/forge-lang/forge.git
cd forge
cargo build
cargo test
```

All 189 tests should pass. If they don't, please open an issue.

### Verify your setup

```bash
# Run the test suite
cargo test

# Run clippy (linter)
cargo clippy

# Run examples
./target/debug/forge run examples/hello.fg
./target/debug/forge run examples/showcase.fg

# Start the REPL
./target/debug/forge
```

## Architecture

Forge is a programming language implemented in Rust. Here's how the pieces fit together:

```
Source (.fg) → Lexer → Tokens → Parser → AST → Interpreter → Result
                                           ↓
                                      Type Checker
                                           ↓
                                    Bytecode Compiler (--vm)
                                           ↓
                                     Register VM + GC
```

### Module Map

```
src/
├── main.rs              # CLI entry point (clap)
├── lexer/
│   ├── token.rs         # Token enum — every atom of the language
│   └── lexer.rs         # Hand-rolled lexer, string interpolation
├── parser/
│   ├── ast.rs           # AST node definitions (Stmt, Expr, Pattern)
│   └── parser.rs        # Recursive descent parser, Pratt precedence
├── interpreter/
│   └── mod.rs           # Tree-walk interpreter, builtins, environment
├── vm/
│   ├── compiler.rs      # AST → bytecode
│   ├── machine.rs       # Register-based VM execution
│   ├── gc.rs            # Mark-sweep garbage collector
│   ├── value.rs         # VM value types
│   ├── bytecode.rs      # Instruction set
│   ├── frame.rs         # Call frames
│   └── green.rs         # Green thread scheduler (scaffold)
├── stdlib/
│   ├── math.rs          # sqrt, pow, abs, sin, cos, random, etc.
│   ├── fs.rs            # read, write, list, mkdir, copy, etc.
│   ├── io.rs            # prompt, print, args
│   ├── crypto.rs        # sha256, md5, base64, hex
│   ├── db.rs            # SQLite (open, query, execute, close)
│   ├── pg.rs            # PostgreSQL (connect, query, execute, close)
│   ├── env.rs           # Environment variables
│   ├── json_module.rs   # parse, stringify, pretty
│   ├── regex_module.rs  # test, find, replace, split
│   ├── log.rs           # info, warn, error, debug
│   ├── exec_module.rs   # run_command
│   ├── term.rs          # Colors, tables, sparklines, UI widgets
│   ├── http.rs          # HTTP client, download, crawl
│   └── csv.rs           # parse, stringify, read, write
├── runtime/
│   ├── server.rs        # HTTP server (axum), route extraction
│   └── client.rs        # HTTP client (reqwest)
├── repl/
│   └── mod.rs           # REPL with rustyline, history, completion
├── lsp/
│   └── mod.rs           # Language Server Protocol
├── testing/
│   └── mod.rs           # Test runner for @test functions
├── typechecker.rs       # Gradual type checking
├── formatter.rs         # forge fmt
├── scaffold.rs          # forge new
├── package.rs           # forge install
├── manifest.rs          # forge.toml parsing
├── learn.rs             # Interactive tutorials
├── chat.rs              # AI chat mode
└── errors.rs            # Error formatting with ariadne
```

## How to Make Changes

### Adding a new keyword

1. Add the token variant to `Token` enum in `src/lexer/token.rs`
2. Add the string-to-token mapping in `keyword_from_str()` in the same file
3. Add parsing logic in `src/parser/parser.rs` (usually in `parse_statement()`)
4. Add an AST node in `src/parser/ast.rs` if it needs new syntax
5. Add execution logic in `src/interpreter/mod.rs` (in `exec_stmt()` or `eval_expr()`)
6. Add a test

### Adding a new builtin function

1. Add the function name to `register_builtins()` in `src/interpreter/mod.rs`
2. Add a match arm in `call_builtin()` in the same file
3. Add a test
4. That's it — it's immediately available in Forge code

### Adding a new stdlib module

1. Create `src/stdlib/mymodule.rs`
2. Add `pub mod mymodule;` to `src/stdlib/mod.rs`
3. Add a `create_module()` function that returns `Vec<(&str, Value)>`
4. Register it in `register_builtins()` in the interpreter
5. Add tests

### Adding a new HTTP route method

1. Add a match arm in `extract_routes()` in `src/runtime/server.rs`
2. Add axum route registration in `start_server()` in the same file
3. Add a test

### Adding a new operator

1. Add the token variant in `src/lexer/token.rs`
2. Add lexing in `src/lexer/lexer.rs`
3. Add to the appropriate precedence level in the parser
4. Add a `BinOp` variant in `src/parser/ast.rs`
5. Add evaluation in `eval_binop()` in `src/interpreter/mod.rs`
6. Add a test

## Code Style

- **Rust 2021 edition**, targets Rust 1.85+
- **No `unwrap()` in production code** — use `?` or proper error handling
- **No `unsafe` code** — the entire codebase is safe Rust
- Keep modules focused: one concept per file
- Tests go in the same file as the code (`#[cfg(test)]` module)
- Use `IndexMap<String, Value>` for Forge objects (preserves insertion order)
- Error messages should be helpful — include suggestions when possible

### Naming Conventions

- Rust: standard snake_case for functions, PascalCase for types
- Forge builtins: snake_case (e.g., `run_command`, `base64_encode`)
- Forge modules: lowercase (e.g., `math`, `fs`, `crypto`)
- Test functions: descriptive names (e.g., `test_array_map_filter`)

## Testing

### Run all tests

```bash
cargo test
```

### Run a specific test

```bash
cargo test test_name
```

### Run Forge integration tests

```bash
./target/debug/forge test tests/
```

### Writing tests

Tests live in `#[cfg(test)]` modules at the bottom of each source file. Use the helper pattern:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn run(code: &str) -> String {
        // helper that captures output
    }

    #[test]
    fn test_my_feature() {
        let output = run("say \"hello\"");
        assert!(output.contains("hello"));
    }
}
```

For Forge-level tests, create `.fg` files in `tests/` with `@test` functions:

```
@test
define my_test() {
    assert(1 + 1 == 2)
    assert_eq(len("forge"), 5)
}
```

## Pull Request Process

1. **Fork and branch** — create a feature branch from `main`
2. **Write code** — follow the code style guidelines above
3. **Add tests** — every new feature or bugfix needs tests
4. **Run the full suite** — `cargo test && cargo clippy`
5. **Run examples** — `forge run examples/showcase.fg` as a smoke test
6. **Write a clear PR description** — what, why, and how to test

### PR Checklist

- [ ] `cargo test` passes (all 189+ tests)
- [ ] `cargo clippy` has no new warnings
- [ ] New features have tests
- [ ] Examples still work (`forge run examples/showcase.fg`)
- [ ] Code follows the style guide (no `unwrap()`, no `unsafe`)

## Issue Reporting

When filing a bug report, please include:

1. **Forge version** (`forge version`)
2. **OS and Rust version** (`rustc --version`)
3. **Minimal reproduction** — the smallest `.fg` file that triggers the bug
4. **Expected vs actual behavior**
5. **Full error output**

## Areas for Contribution

### Good first issues

- Fix clippy warnings (see `cargo clippy` output)
- Add doc comments to stdlib functions
- Improve error messages with more context
- Add more examples in `examples/`

### Larger projects

- **VM feature parity** — the bytecode VM doesn't support all interpreter features yet
- **Standard library expansion** — `net`, `time`, `path`, `testing` modules
- **Formatter improvements** — `forge fmt` handles basic indentation; needs more
- **LSP completion** — the LSP server is scaffolded but needs intellisense
- **Package registry** — `forge install` works with git URLs; needs a registry

## License

By contributing to Forge, you agree that your contributions will be licensed under the MIT License.
