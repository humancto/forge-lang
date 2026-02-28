# Changelog

All notable changes to Forge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-28

### Added

- **Bytecode VM** with register-based architecture, mark-sweep GC, and green thread scheduler
- **Natural language syntax**: `set`/`to`, `say`/`yell`/`whisper`, `define`, `repeat`, `otherwise`/`nah`, `grab`/`toss`, `for each`
- **15 standard library modules**: math, fs, io, crypto, db (SQLite), pg (PostgreSQL), env, json, regex, log, exec, term, http, csv
- **Terminal UI toolkit**: colors, tables, sparklines, bars, banners, progress, gradients, boxes, typewriter effects
- **HTTP server** with `@server`, `@get`, `@post`, `@put`, `@delete`, `@ws` decorators (powered by axum)
- **HTTP client** with `fetch()`, `http.get/post/put/delete/patch/head`, `download`, `crawl`
- **Shell integration**: `shell()` for full pipe chain support, `sh()` shorthand
- **Innovation features**: `when` guards, `must` keyword, `safe` blocks, `check` validation, `retry`/`timeout`/`schedule`/`watch` blocks
- **AI integration**: `ask()` for LLM calls, `prompt` templates, `agent` blocks
- **Developer tools**: `forge fmt`, `forge test`, `forge new`, `forge build`, `forge install`, `forge lsp`, `forge learn`, `forge chat`
- **Interactive tutorial system** with 14 lessons
- **Type checker** with gradual type checking and warnings
- **Algebraic data types** with pattern matching
- **Result/Option types** with `?` operator propagation
- **LSP server** for editor integration
- **Package manager** for git-based and local package installation
- **189 tests** (Rust unit + Forge integration)

### Changed

- Default execution engine switched from VM to interpreter for broader feature support
- VM available via `--vm` flag for performance-critical workloads
- Improved error messages with "did you mean?" suggestions and source context
- REPL upgraded with rustyline (history, completion, multiline)

## [0.1.0] - 2026-01-15

### Added

- Initial release
- Lexer with string interpolation
- Recursive descent parser
- Tree-walk interpreter
- Basic HTTP server and client
- REPL
- 7 example programs
