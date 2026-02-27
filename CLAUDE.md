# CLAUDE.md — Forge Language Project Context

## What Is This?

Forge is an internet-native programming language built in Rust. Think "Go's simplicity + Rust's safety + HTTP as a language primitive." The compiler is a tree-walk interpreter (Phase 1) that will become a bytecode VM (Phase 3).

## Architecture

```
Source (.fg) → Lexer → Tokens → Parser → AST → Interpreter → Result
                                                    ↓
                                              Runtime Bridge
                                            (axum, reqwest, tokio)
```

### Module Map

| File | Purpose | Lines |
|------|---------|-------|
| `src/main.rs` | CLI entry point, `forge run` / `forge repl` | ~140 |
| `src/lexer/token.rs` | Token enum — every atom of the language | ~140 |
| `src/lexer/lexer.rs` | Hand-rolled lexer with string interpolation | ~350 |
| `src/parser/ast.rs` | AST node definitions (Stmt, Expr, Pattern, etc.) | ~260 |
| `src/parser/parser.rs` | Recursive descent parser with Pratt precedence | ~810 |
| `src/interpreter/mod.rs` | Tree-walk interpreter, builtins, environment | ~960 |
| `src/runtime/server.rs` | HTTP server powered by axum + tokio | ~240 |
| `src/runtime/client.rs` | HTTP client powered by reqwest | ~100 |
| `src/repl/mod.rs` | Interactive REPL with multiline support | ~165 |
| `src/errors.rs` | Error formatting (to be replaced by ariadne) | ~60 |

### Key Design Decisions

- **No OOP.** Structs + interfaces (Go-style implicit satisfaction). No classes, no inheritance.
- **Errors are values.** `Result<T, E>` with `?` propagation. No try/catch. No exceptions.
- **Null doesn't exist.** `Option<T>` is the only nullable path (not yet implemented).
- **Newline-terminated.** No semicolons required (like Go).
- **Immutable by default.** `let x = 5` is immutable, `let mut x = 5` is mutable.
- **String interpolation.** `"Hello, {name}!"` — curly braces in strings are interpolated.
- **Decorators for HTTP.** `@get("/path")`, `@post("/path")`, `@server(port: 8080)`.
- **File extension:** `.fg`

### How Things Flow

**Adding a new keyword:**
1. Add variant to `Token` enum in `src/lexer/token.rs`
2. Add to `keyword_from_str()` match in same file
3. Add parsing rule in `src/parser/parser.rs` (usually in `parse_statement()`)
4. Add AST node in `src/parser/ast.rs` if needed
5. Add execution logic in `src/interpreter/mod.rs` (in `exec_stmt()` or `eval_expr()`)

**Adding a new builtin function:**
1. Add name to the `register_builtins()` list in `src/interpreter/mod.rs`
2. Add match arm in `call_builtin()` in same file
3. That's it — it's immediately available in Forge code

**Adding a new HTTP route method (e.g., PATCH):**
1. Add match arm in `extract_routes()` in `src/runtime/server.rs`
2. Add axum route registration in `start_server()` in same file

**Adding a new operator:**
1. Add token variant in `src/lexer/token.rs`
2. Add lexing rule in `src/lexer/lexer.rs`
3. Add to appropriate precedence level in parser (e.g., `parse_addition()`)
4. Add `BinOp` variant in `src/parser/ast.rs`
5. Add evaluation in `eval_binop()` in `src/interpreter/mod.rs`

## Tech Stack

| Crate | Purpose | Why |
|-------|---------|-----|
| `axum` | HTTP server | Production-grade, powers Cloudflare/Discord |
| `tokio` | Async runtime | Industry standard, full features |
| `reqwest` | HTTP client | HTTPS via rustls, JSON support |
| `tower-http` | Middleware | CORS, tracing, compression |
| `serde` / `serde_json` | Serialization | Zero-cost JSON |
| `clap` | CLI framework | Not yet wired — use for `forge` subcommands |
| `rustyline` | REPL | Not yet wired — use for tab completion, history |
| `ariadne` | Error reporting | Not yet wired — use for source-mapped diagnostics |
| `chrono` | Time | Not yet wired — use for `time.now()` builtin |
| `uuid` | UUIDs | Not yet wired — use for `uuid()` builtin |

## Build & Test

```bash
cargo build                              # debug build
cargo build --release                    # optimized build
cargo test                               # run all tests (7 lexer tests currently)
./target/debug/forge run examples/api.fg # start HTTP server
./target/debug/forge run examples/hello.fg
./target/debug/forge                     # REPL
```

## Forge Syntax Quick Reference

```
// Variables
let name = "Forge"
let mut counter = 0

// Functions
fn greet(name: String) -> String {
    return "Hello, {name}!"
}

// Closures
let double = fn(x) { return x * 2 }

// Objects (JSON-native)
let user = { name: "Odin", level: 99 }

// Arrays
let nums = [1, 2, 3, 4, 5]

// Control flow
if x > 10 { println("big") } else { println("small") }
for item in items { println(item) }
while running { tick() }

// HTTP server
@server(port: 8080)

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}

// HTTP client
let resp = fetch("http://example.com/api")
println(resp.status)
println(resp.body)

// Pipeline
data |> transform |> filter |> send
```

## Known Limitations (Fix These)

1. **`ariadne` not wired.** Error reporting uses basic ANSI colors instead of source-mapped diagnostics.
2. **`clap` not wired.** CLI uses manual arg parsing instead of derive-based subcommands.
3. **`rustyline` not wired.** REPL uses raw stdin instead of readline with history/completion.
4. **`chrono` not wired.** `time` builtin returns a hardcoded stub.
5. **`uuid` not wired.** No `uuid()` builtin yet.
6. **No type checking.** Type annotations are parsed but not enforced.
7. **`spawn` is synchronous.** Green threads not implemented — `spawn {}` just runs the block.
8. **No algebraic data types.** `type Shape = Circle(f64) | Rect(f64, f64)` parses but doesn't execute.
9. **Object key order is random.** HashMap doesn't preserve insertion order — use IndexMap.
10. **Pattern matching is basic.** Constructor patterns only fully destructure Result values.
11. **No standard library modules.** No `io`, `fs`, `math`, `crypto` modules yet.
12. **No `forge.toml` / package manifest.**
13. **No formatter (`forge fmt`).**
14. **No test runner (`forge test`).**

## Roadmap

### Phase 2 — Type System & Error Handling (Next)
- Gradual type checking (annotations optional, enforced when present)
- `Result<T, E>` as a real type with `Ok(value)` and `Err(message)` constructors
- `?` operator propagates errors up the call stack
- Algebraic data types with exhaustive pattern matching
- Interfaces (Go-style implicit satisfaction)
- Pipeline operator `|>` with proper type flow

### Phase 3 — Bytecode VM
- Register-based VM replacing tree-walk interpreter
- Bytecode compiler (AST → bytecode)
- Garbage collector (tri-color mark-sweep)
- Green threads on tokio (real `spawn`)
- 20-50x speedup over tree-walking

### Phase 4 — Production
- Standard library (io, fs, http, json, db, crypto, time, testing)
- Database integration (sqlx — Postgres, SQLite, MySQL)
- `forge new`, `forge test`, `forge fmt`, `forge build`
- Package manifest (`forge.toml`)

## Code Style

- Rust 2021 edition, targets Rust 1.85+
- No `unwrap()` in production paths — use `?` or proper error handling
- Keep modules focused: one concept per file
- Tests go in the same file as the code (`#[cfg(test)]` mod)
- AST nodes use `enum` with named fields, not tuple variants
- Use `HashMap<String, Value>` for Forge objects (switch to IndexMap later)
- Errors carry line/col for source mapping

## When Modifying

- **Always run `cargo test` after changes**
- **Test with `examples/hello.fg` and `examples/functional.fg`** for language features
- **Test with `examples/api.fg`** for HTTP server changes
- The interpreter's `Environment` uses a scope stack (Vec of HashMaps) — push/pop for blocks
- Function closures capture the environment at definition time — watch for clone overhead
- The parser uses `skip_newlines()` liberally — Forge is newline-aware but not newline-sensitive
