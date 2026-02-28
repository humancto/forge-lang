# RFC 0001: Language Vision and Core Principles

- **Status:** Implemented
- **Author:** Archith Rapaka
- **Date:** 2026-01-15

## Summary

Forge is a programming language designed for building internet software without dependency sprawl. HTTP, databases, cryptography, and terminal UI are built into the language runtime — not added through packages.

## Motivation

Modern backend development requires assembling a tower of dependencies before writing application logic:

```
pip install flask requests sqlalchemy bcrypt python-dotenv pydantic redis celery gunicorn
```

Each dependency introduces version conflicts, supply chain risk, API churn, and cognitive overhead. Yet the capabilities they provide — HTTP, database access, JSON, crypto — are what **every** internet application needs.

The insight behind Forge: these capabilities should be **language primitives**, not library features.

## Design Principles

### 1. Internet-Native

HTTP is not a framework. It's a language construct:

```
@server(port: 3000)

@get("/users/:id")
fn get_user(id: String) -> Json {
    return db.query("SELECT * FROM users WHERE id = " + id)
}
```

Database, crypto, JSON, regex, file I/O, and terminal UI follow the same principle — always available, always documented, always tested.

### 2. Human-Readable

Code should read close to intent. Forge provides two syntaxes that coexist:

```
// Both of these are valid and identical in behavior:
let name = "World"           set name to "World"
println("Hello!")            say "Hello!"
fn greet() { }               define greet() { }
if x { } else { }            if x { } otherwise { }
```

### 3. Errors Are Values

No exceptions. No invisible control flow. Functions that can fail return `Result` types. The `?` operator propagates errors. The `must` keyword asserts success.

### 4. Immutable by Default

`let x = 5` is immutable. You must explicitly opt into mutability with `let mut x = 5`. This prevents an entire class of bugs and makes code easier to reason about.

### 5. No Null

There is no `null` value that silently propagates through your program. `Option<T>` is the only nullable path, and it requires explicit handling.

### 6. Safe Rust Foundation

The entire Forge implementation uses zero `unsafe` blocks. Memory safety, thread safety, and crash resistance come from the Rust foundation.

## Alternatives Considered

### "Why not just use Python/Node.js with good libraries?"

Libraries are external. They have their own release cycles, breaking changes, and security vulnerabilities. Language primitives are stable, documented, and tested as part of the language itself.

### "Why not compile to WASM/LLVM?"

Phase 1 prioritizes developer experience over raw performance. A tree-walk interpreter and bytecode VM provide fast iteration. LLVM compilation is a potential Phase 5 target.

### "Why Rust as the implementation language?"

Rust provides memory safety without garbage collection, fearless concurrency, and excellent ecosystem (axum, tokio, reqwest). It's the ideal foundation for a language runtime that handles HTTP, databases, and crypto.
