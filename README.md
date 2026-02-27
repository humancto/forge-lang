# 🔥 Forge

**Go's simplicity. Rust's safety. The internet built in.**

Forge is an internet-native programming language built in Rust. HTTP, JSON, and concurrency are language primitives — not library imports.

## Quick Start

```bash
# Build
cargo build --release

# Run a program
./target/release/forge run examples/hello.fg

# Start the REPL
./target/release/forge
```

## Examples

### Hello World
```
let name = "World"
println("Hello, {name}!")
```

### Functions & Closures
```
fn make_adder(n) {
    return fn(x) { return x + n }
}

let add5 = make_adder(5)
println(add5(10))  // 15
```

### Objects (JSON-native)
```
let user = {
    name: "Odin",
    role: "Builder",
    level: 99
}
println(user)
```

### Result + `?` Propagation
```
fn parse_num(s) {
    if s == "" { return Err("empty input") }
    return Ok(int(s))
}

fn add_one(s) {
    let n = parse_num(s)?
    return Ok(n + 1)
}

println(add_one("41"))  // Ok(42)
println(add_one(""))    // Err(empty input)
```

### API Server
```
@server(port: 8080)

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}
```

## Project Status

**Phase 1 — Foundation** ✅ In Progress

| Component | Status | Lines |
|-----------|--------|-------|
| Lexer | ✅ Complete | 346 |
| Parser | ✅ Complete | 808 |
| AST | ✅ Complete | 259 |
| Interpreter | ✅ Complete | 948 |
| REPL | ✅ Complete | 164 |
| Error Formatting | ✅ Complete | 58 |
| CLI | ✅ Complete | 120 |
| **Total** | | **2,850** |

### What Works
- Variables (`let`, `mut`)
- Functions (with closures, recursion, higher-order)
- String interpolation with expressions (`"sum = {a + b}"`, `"{user.name}"`)
- Arithmetic, comparison, logical operators
- Arrays and objects (JSON-native)
- Control flow (`if/else`, `for..in`, `while`, `loop`)
- Pattern matching (basic)
- Result values (`Ok`, `Err`) with `?` unwrapping/propagation
- Pipeline operator (`|>`)
- Decorators (`@server`, `@get`, `@post`)
- HTTP client (`fetch()`)
- HTTP server routing (`@server`, `@get`, `@post`, `@put`, `@delete`)
- REPL with multiline support
- Source-mapped error reporting

### Coming Next
- [ ] Full JSON parsing (`json.parse/stringify`)
- [ ] Type annotations
- [ ] Algebraic data types

## Architecture

```
Source (.fg) → Lexer → Tokens → Parser → AST → Interpreter → Result
```

Built on the "Ride on Giants" philosophy — Forge delegates heavy lifting to battle-tested Rust crates:

| Feature | Rust Crate | Status |
|---------|-----------|--------|
| HTTP Server | hyper + tower | Phase 1b |
| HTTP Client | reqwest | Phase 1b |
| Async Runtime | tokio | Phase 3 |
| JSON | serde_json | ✅ Integrated |
| TLS | rustls | Phase 4 |
| Database | sqlx | Phase 4 |

## License

MIT
