# Forge Roadmap

This document outlines the planned evolution of Forge. Priorities may shift based on community feedback.

---

## v0.2 — Foundation (Current Release)

- [x] Tree-walk interpreter with full feature support
- [x] Bytecode VM with register-based architecture and mark-sweep GC
- [x] 15 standard library modules (math, fs, crypto, db, pg, json, regex, csv, env, log, term, http, io, exec)
- [x] 160+ built-in functions
- [x] Natural language syntax alongside classic syntax
- [x] HTTP server with decorator-based routing (@get, @post, @put, @delete, @ws)
- [x] HTTP client (fetch, http.get/post/put/delete)
- [x] Shell integration (sh, shell, sh_lines, sh_json, sh_ok, which, pipe_to)
- [x] Object helpers (pick, omit, merge, get with dot-paths, has_key, find, entries)
- [x] Method chaining on arrays and objects
- [x] SQLite and PostgreSQL built in
- [x] Result types with ? propagation
- [x] Algebraic data types with pattern matching
- [x] REPL with history and completion
- [x] Test runner, formatter, project scaffolding
- [x] LSP server
- [x] 14 interactive tutorials
- [x] 189 tests, zero unsafe blocks

## v0.3 — Type System & VM Parity

- [ ] Gradual type checking enforcement (annotations become meaningful)
- [ ] VM feature parity with interpreter (closures, ADTs, method chaining)
- [ ] Generic types (`Array<Int>`, `Map<String, Value>`)
- [ ] Interface satisfaction checking at runtime
- [ ] Improved error messages with source-mapped stack traces
- [ ] `Option<T>` as a proper type (not an object wrapper)

## v0.4 — Concurrency & Performance

- [ ] Green threads via tokio (real `spawn` with message passing)
- [ ] Async/await that actually executes concurrently
- [ ] Channel-based communication between spawned tasks
- [ ] VM performance optimization (20-50x over tree-walk target)
- [ ] Instruction-level profiling

## v0.5 — Package Ecosystem

- [ ] `forge.toml` dependency declaration
- [ ] Package registry (forge-packages.dev or similar)
- [ ] `forge install <package>` from registry
- [ ] Semantic versioning and dependency resolution
- [ ] `forge publish` for package authors

## v0.6 — Standard Library Expansion

- [ ] `net` — TCP/UDP sockets, DNS
- [ ] `time` — dates, durations, formatting, timezones
- [ ] `path` — cross-platform path manipulation
- [ ] `testing` — built-in assertion library, mocks, benchmarks
- [ ] `template` — HTML/text templating engine
- [ ] `websocket` — client-side WebSocket connections

## v0.7 — Developer Experience

- [ ] LSP completion, hover, go-to-definition
- [ ] VS Code extension with syntax highlighting + LSP
- [ ] `forge doc` — documentation generator
- [ ] `forge bench` — benchmarking tool
- [ ] Debugger integration (DAP protocol)
- [ ] REPL with syntax highlighting

## v1.0 — Production

- [ ] Language specification document
- [ ] Stable API guarantee
- [ ] Cross-compilation targets
- [ ] Windows support
- [ ] Prebuilt binaries for all major platforms
- [ ] Homebrew formula
- [ ] Docker image

---

## Design Principles (Won't Change)

These decisions are permanent:

1. **Internet-native** — HTTP, databases, crypto stay in the language, not in packages
2. **Human-readable** — natural syntax is a first-class citizen, not a gimmick
3. **Errors are values** — no exceptions, no invisible control flow
4. **Immutable by default** — `let` is immutable, `let mut` opts in
5. **No null** — `Option<T>` is the only nullable path
6. **No unsafe** — the entire codebase will remain safe Rust
7. **No OOP** — structs + interfaces, no classes, no inheritance

---

## How to Influence the Roadmap

- Open an issue with the `feature-request` label
- Submit an RFC in the `rfcs/` directory
- Join the discussion on existing roadmap issues

Priorities are driven by what makes Forge more useful for building real internet software.
