# CLAUDE.md — Forge Language Project Context

## What Is This?

Forge is an internet-native programming language built in Rust. ~26,000 lines. Dual syntax (classic + natural language). Built-in HTTP, database, crypto, AI, CSV, terminal UI, shell integration, NPC fake data, GenZ debug kit, and 30 interactive tutorials.

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
forge learn                  # 30 interactive tutorials
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

## Standard Library (16 modules, 230+ functions)

| Module   | Key Functions                                                                                    |
| -------- | ------------------------------------------------------------------------------------------------ |
| `math`   | sqrt, pow, abs, max, min, floor, ceil, round, pi, e, sin, cos, random_int, clamp                |
| `fs`     | read, write, append, exists, list, remove, mkdir, copy, rename, size, ext, read_json, write_json, lines, dirname, basename, join_path, is_dir, is_file, temp_dir |
| `io`     | prompt, print, args, args_parse, args_get, args_has                                              |
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
| `npc`    | name, first_name, last_name, email, username, phone, number, pick, bool, sentence, word, id, color, ip, url, company |

## Core Builtins (beyond modules)

- Output: print, println, say, yell, whisper
- Types: str, int, float, type, typeof
- Collections: len, push, pop, keys, values, contains, range, enumerate, sum, min_of, max_of, unique, zip, flatten, group_by, chunk, slice, partition
- Functional: map, filter, reduce, sort (with custom comparator), reverse, find, flat_map, any, all, sample, shuffle
- Objects: has_key, get (with dot-paths), pick, omit, merge, entries, from_entries, diff
- Strings: split, join, replace, starts_with, ends_with, lines, substring, index_of, last_index_of, pad_start, pad_end, capitalize, title, repeat_str, count, slugify, snake_case, camel_case
- Results: Ok, Err, is_ok, is_err, unwrap, unwrap_or
- Options: Some, None, is_some, is_none
- Shell: sh, shell, sh_lines, sh_json, sh_ok, which, cwd, cd, pipe_to
- System: time, uuid, exit, input, wait, run_command
- Validation: assert, assert_eq, assert_ne, assert_throws, satisfies
- GenZ Debug Kit: sus (inspect), bruh (panic), bet (assert), no_cap (assert_eq), ick (assert-false)
- Execution: cook (profiling), yolo (fire-and-forget), ghost (silent exec), slay (benchmarking)
- Concurrency: channel, send, receive, try_send, try_receive

## Build & Test

```bash
cargo build          # 0 errors
cargo test           # 488 Rust tests pass
forge test           # 334 Forge tests pass (822 total)
```

All 12 example files run successfully.

## Known Limitations (v0.3.0)

- SQL queries use raw strings (no parameterized query API yet) — be cautious with user input
- Interpreter is ~20x slower than Python for deep recursion; use `--jit` (11x faster than Python) or `--vm` (2x slower than Python)
- VM/JIT support fewer features than the interpreter — use interpreter (default) for full stdlib, HTTP, DB access
- `regex` functions take `(text, pattern)` order, not `(pattern, text)`
- Result constructors accept both cases: `Ok(42)`/`ok(42)`, `Err("msg")`/`err("msg")`

## Engineering Discipline

These rules are non-negotiable. Follow them on every change.

### Before Every Change

1. **Read the code you're modifying.** Never edit blind.
2. **Run `cargo test` before starting.** Know what passes now.
3. **Understand the dependency chain.** Changing `bytecode.rs` affects `compiler.rs`, `machine.rs`, `ir_builder.rs`, and `serialize.rs`.

### During Changes

4. **Small, atomic commits.** One concern per commit. Never mix features.
5. **Tests before or alongside code.** Risky changes get tests first.
6. **No `unwrap()` in production paths.** Use `?` or proper error handling.
7. **If it compiles but feels wrong, stop.** Check the design.
8. **Never remove a working execution path.** Interpreter, VM, and JIT must all keep working.

### After Every Change

9. **Run `cargo test`.** If tests fail, fix before committing.
10. **Run the examples.** `forge run examples/hello.fg` and `forge run examples/functional.fg` must pass.
11. **Check for regressions.** If you changed the VM, test with `--vm`. If you changed the JIT, test with `--jit`.

### Learnings (Append Here)

- **JIT jump offsets:** The VM pre-increments IP before applying jump offsets. JIT target = `ip + 1 + sbx`, not `ip + sbx`. This caused fib(30) to return wrong values.
- **Builtin shadowing:** Registering a `BuiltIn("time")` after a `time` module object shadows the module. Register modules last, or remove the simple builtin.
- **Value PartialEq:** The interpreter's `Value` enum needs a manual `PartialEq` impl because `Function`/`Lambda` variants contain non-comparable closures. Never derive it.
- **GitHub Actions runners:** `macos-13` is deprecated. Use `macos-latest` for both ARM and x86_64 targets.
- **Bytecode encoding:** Instructions are 32-bit. Format: `[op:8][a:8][b:8][c:8]` or `[op:8][a:8][bx:16]` or `[op:8][a:8][sbx:16]`. The `sbx` field is signed 16-bit stored as unsigned.
- **Constant dedup:** `Chunk::add_constant()` deduplicates via `identical()`. Don't add the same constant twice — it wastes the constant pool.

## Module Dependency Map

```
main.rs → lexer, parser, interpreter, vm, runtime, errors, typechecker, ...
vm/mod.rs → compiler, machine, bytecode, frame, gc, green, jit, value
vm/machine.rs → bytecode, frame, gc, value (2483 lines — largest VM file)
vm/compiler.rs → bytecode, parser::ast (927 lines)
vm/jit/ir_builder.rs → bytecode, cranelift (276 lines)
vm/jit/jit_module.rs → ir_builder (47 lines)
interpreter/mod.rs → parser::ast, runtime, stdlib (8153 lines — largest file)
runtime/server.rs → interpreter, parser::ast, axum (354 lines)
runtime/client.rs → reqwest (100 lines)
```
