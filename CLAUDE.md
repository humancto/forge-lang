# CLAUDE.md — Forge Language Project Context

## What Is This?

Forge is an internet-native programming language built in Rust. ~15,500 lines. Dual syntax (classic + natural language). Built-in HTTP, database, crypto, AI, CSV, terminal UI, shell integration, and 14 interactive tutorials.

## Architecture

```
Source (.fg) → Lexer → Parser → AST → Type Checker → Interpreter → Result
                                                         ↓
                                                  Runtime Bridge
                                              (axum, reqwest, tokio, rusqlite)
```

The interpreter is the default engine. A bytecode VM (`--vm` flag) exists for performance-critical workloads but supports fewer features.

## Quick Start

```bash
cargo build
forge learn                  # 14 interactive tutorials
forge run examples/hello.fg  # run a program
forge -e 'say "hello!"'     # eval inline
forge new my-app             # scaffold project
forge test                   # run tests
forge chat                   # AI chat mode
forge fmt                    # format code
```

## Dual Syntax (Classic + Natural)

| Feature     | Classic            | Forge-Unique                |
| ----------- | ------------------ | --------------------------- |
| Variables   | `let x = 5`        | `set x to 5`                |
| Mutable     | `let mut x = 0`    | `set mut x to 0`            |
| Reassign    | `x = 10`           | `change x to 10`            |
| Functions   | `fn add(a, b) { }` | `define add(a, b) { }`      |
| Output      | `println("hi")`    | `say` / `yell` / `whisper`  |
| Else        | `else { }`         | `otherwise { }` / `nah { }` |
| Async fn    | `async fn x() { }` | `forge x() { }`             |
| Await       | `await expr`       | `hold expr`                 |
| Yield       | `yield value`      | `emit value`                |
| Destructure | `let {a, b} = obj` | `unpack {a, b} from obj`    |
| Fetch       | `fetch("url")`     | `grab resp from "url"`      |

## Innovation Keywords (unique to Forge)

- `when age { < 13 -> "kid", else -> "senior" }` -- when guards
- `must expr` -- crash on error with clear message
- `safe { risky_code() }` -- null-safe execution (statement only)
- `check name is not empty` -- declarative validation
- `retry 3 times { }` -- automatic retry
- `timeout 5 seconds { }` -- time-limited execution (experimental)
- `schedule every 5 minutes { }` -- cron tasks
- `watch "file" { }` -- file change detection
- `ask "prompt"` -- AI/LLM calls
- `download "url" to "file"` -- file download
- `crawl "url"` -- web scraping
- `repeat 5 times { }` -- counted loop
- `wait 2 seconds` -- sleep with units

## CLI Commands (13)

run, repl, version, fmt, test, new, build, install, lsp, learn, chat, help, -e

## Standard Library (15 modules, 160+ functions)

| Module   | Key Functions                                                                                    |
| -------- | ------------------------------------------------------------------------------------------------ |
| `math`   | sqrt, pow, abs, max, min, floor, ceil, round, pi, e, sin, cos                                    |
| `fs`     | read, write, append, exists, list, remove, mkdir, copy, rename, size, ext, read_json, write_json |
| `io`     | prompt, print, args                                                                              |
| `crypto` | sha256, md5, base64_encode/decode, hex_encode/decode                                             |
| `db`     | open, query, execute, close (SQLite)                                                             |
| `pg`     | connect, query, execute, close (PostgreSQL)                                                      |
| `env`    | get, set, has, keys                                                                              |
| `json`   | parse, stringify, pretty                                                                         |
| `regex`  | test(text, pattern), find, find_all, replace, split                                              |
| `log`    | info, warn, error, debug                                                                         |
| `http`   | get, post, put, delete, patch, head, download, crawl                                             |
| `csv`    | parse, stringify, read, write                                                                    |
| `term`   | red/green/blue/yellow/bold/dim, table, hr, sparkline, bar, banner, box, gradient, success/error  |
| `exec`   | run_command                                                                                      |

## Core Builtins (beyond modules)

- Output: print, println, say, yell, whisper
- Types: str, int, float, type, typeof
- Collections: len, push, pop, keys, values, contains, range, enumerate
- Functional: map, filter, reduce, sort, reverse, find, flat_map
- Objects: has_key, get (with dot-paths), pick, omit, merge, entries, from_entries
- Strings: split, join, replace, starts_with, ends_with, lines
- Results: Ok, Err, is_ok, is_err, unwrap, unwrap_or
- Options: Some, None, is_some, is_none
- Shell: sh, shell, sh_lines, sh_json, sh_ok, which, cwd, cd, pipe_to
- System: time, uuid, exit, input, wait, run_command
- Validation: assert, assert_eq, satisfies

## Build & Test

```bash
cargo build          # 0 warnings, 0 errors
cargo test           # 189 tests pass
forge test           # 25 Forge tests pass
```

All 12 example files run successfully.

## Known Limitations (v0.2.0)

- SQL queries use raw strings (no parameterized query API yet) — be cautious with user input
- Interpreter is ~20x slower than Python for deep recursion; use `--jit` (11x faster than Python) or `--vm` (2x slower than Python)
- VM/JIT support fewer features than the interpreter — use interpreter (default) for full stdlib, HTTP, DB access
- `regex` functions take `(text, pattern)` order, not `(pattern, text)`
- Result constructors accept both cases: `Ok(42)`/`ok(42)`, `Err("msg")`/`err("msg")`
