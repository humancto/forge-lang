<p align="center">
  <h1 align="center">⚒️ Forge</h1>
  <p align="center"><strong>The internet-native programming language.</strong></p>
  <p align="center">Built-in HTTP, databases, crypto, AI, and a JIT compiler. Write less. Build more.</p>
</p>

<p align="center">
  <a href="https://github.com/humancto/forge-lang/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/humancto/forge-lang/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/humancto/forge-lang/releases"><img alt="Release" src="https://img.shields.io/github/v/release/humancto/forge-lang?style=flat-square&color=blue"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-green?style=flat-square"></a>
  <a href="https://www.rust-lang.org/"><img alt="Built with Rust" src="https://img.shields.io/badge/built_with-Rust-orange?style=flat-square"></a>
  <img alt="Tests" src="https://img.shields.io/badge/tests-287_passing-brightgreen?style=flat-square">
  <a href="https://github.com/humancto/forge-lang/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/humancto/forge-lang?style=flat-square"></a>
  <a href="https://crates.io/crates/forge-lang"><img alt="crates.io" src="https://img.shields.io/crates/v/forge-lang?style=flat-square"></a>
</p>

---

## Table of Contents

- [Quick Example](#quick-example)
- [What Is Forge?](#what-is-forge)
- [Installation](#installation)
- [Why Forge?](#why-forge)
- [Quick Tour](#quick-tour)
- [Performance](#performance)
- [Standard Library](#standard-library)
- [CLI](#cli)
- [Examples](#examples)
- [Architecture](#architecture)
- [Editor Support](#editor-support)
- [Project Status](#project-status)
- [Known Limitations](#known-limitations-v020)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Community](#community)
- [License](#license)

---

## Quick Example

A REST API in Forge:

```forge
@server(port: 3000)

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}
```

```bash
forge run api.fg
curl http://localhost:3000/hello/World
# → {"greeting": "Hello, World!"}
```

No framework. No dependencies. No setup.

---

## What Is Forge?

Forge is a programming language where HTTP, databases, crypto, and terminal UI are **built into the runtime** — not added through packages.

```forge
say "Hello, World!"

let users = db.query("SELECT * FROM users")
term.table(users)

let hash = crypto.sha256("password")
say hash

let resp = fetch("https://api.example.com/data")
say resp.json.name
```

Every line above runs without a single import or install. 15 standard library modules, 160+ built-in functions, zero external dependencies needed.

It also reads like English — or like code. Both work:

```forge
// Natural syntax                        // Classic syntax
set name to "Forge"                      let name = "Forge"
say "Hello, {name}!"                     println("Hello, {name}!")
define greet(who) { }                    fn greet(who) { }
if ready { } otherwise { }              if ready { } else { }
repeat 3 times { }                       for i in range(0, 3) { }
```

---

## Installation

**Latest: v0.2.0** — install via any method below.

### Cargo (Rust)

```bash
cargo install forge-lang
```

Requires [Rust 1.85+](https://rustup.rs/). Published on [crates.io](https://crates.io/crates/forge-lang).

### Homebrew (macOS & Linux)

```bash
brew install humancto/tap/forge
```

### Install script

```bash
curl -fsSL https://raw.githubusercontent.com/humancto/forge-lang/main/install.sh | bash
```

### From source

```bash
git clone https://github.com/humancto/forge-lang.git
cd forge-lang
cargo install --path .
```

### Verify

```bash
forge version          # check installation
forge learn            # 14 interactive tutorials
forge                  # start REPL
```

---

## Why Forge?

Modern backend development requires installing dozens of packages before writing a single line of logic:

```bash
pip install flask requests sqlalchemy bcrypt python-dotenv pydantic ...
```

Forge:

```bash
forge run app.fg
```

| Problem                     | Forge                                                   |
| --------------------------- | ------------------------------------------------------- |
| HTTP requires a framework   | `@server` + `@get` — 3 lines                            |
| Database needs an ORM       | `db.query("SELECT * FROM users")` — built in            |
| Crypto needs a library      | `crypto.sha256("data")` — built in                      |
| JSON needs parsing          | `json.parse(text)` — built in                           |
| CSV needs pandas            | `csv.read("data.csv")` — built in                       |
| Shell scripts are fragile   | `sh("whoami")`, `shell("cmd \| grep x")` — built in     |
| Terminal UIs need ncurses   | `term.table(data)`, `term.sparkline(vals)` — built in   |
| Error handling is bolted on | `Result` types with `?` propagation — it's the language |
| Learning a language is slow | `forge learn` — 14 lessons in your terminal             |

---

## Quick Tour

### Variables

```forge
let name = "Forge"              // immutable
let mut count = 0               // mutable
count += 1

set language to "Forge"         // natural syntax
set mut score to 0
change score to score + 10
```

### Functions

```forge
fn add(a, b) { return a + b }

define greet(name) {
    say "Hello, {name}!"
}

let double = fn(x) { x * 2 }   // implicit return
```

### Output — The Fun Trio

```forge
say "Normal volume"              // standard output
yell "LOUD AND PROUD!"          // UPPERCASE + !
whisper "quiet and gentle"       // lowercase + ...
```

### Control Flow

```forge
if score > 90 { say "A" }
otherwise if score > 80 { say "B" }
otherwise { say "C" }

let label = when temp {
    > 100 -> "Boiling"
    > 60  -> "Warm"
    else  -> "Cold"
}
```

### Loops

```forge
for item in [1, 2, 3] { say item }

for each color in ["red", "green", "blue"] {
    say color
}

repeat 5 times { say "hello" }

while count < 10 { count += 1 }
```

### Collections

```forge
let nums = [1, 2, 3, 4, 5]
let evens = nums.filter(fn(x) { x % 2 == 0 })
let doubled = evens.map(fn(x) { x * 2 })
say doubled   // [4, 8]

let user = { name: "Alice", age: 30, "Content-Type": "json" }
say user.name
say pick(user, ["name"])
say has_key(user, "email")
```

### Error Handling

```forge
fn safe_divide(a, b) {
    if b == 0 { return Err("division by zero") }
    return Ok(a / b)
}

let result = safe_divide(10, 0)
match result {
    Ok(val) => say "Got: {val}"
    Err(msg) => say "Error: {msg}"
}

// Propagate with ?
fn compute(input) {
    let n = parse_int(input)?
    return Ok(n * 2)
}
```

### Innovation Keywords

```forge
safe { risky_function() }                // returns null on error
let r = safe { might_fail() }           // safe as expression
must parse_config("app.toml")            // crash with clear message on error
check email is not empty                 // declarative validation
retry 3 times { fetch("https://api.example.com") }  // automatic retry
timeout 5 seconds { long_operation() }   // enforced time limit
wait 2 seconds                           // sleep with units
```

### Pattern Matching & ADTs

```forge
type Shape = Circle(Float) | Rect(Float, Float)

let s = Circle(5.0)
match s {
    Circle(r) => say "Area = {3.14 * r * r}"
    Rect(w, h) => say "Area = {w * h}"
}
```

---

## Performance

Forge has three execution tiers:

| Engine      | fib(30) | vs Python      | Best For                    |
| ----------- | ------- | -------------- | --------------------------- |
| `--jit`     | 10ms    | **11x faster** | Compute-heavy hot functions |
| `--vm`      | 252ms   | 2.2x slower    | General bytecode execution  |
| Interpreter | 2,300ms | 20x slower     | Full feature set + stdlib   |

The JIT compiles hot functions to native code via [Cranelift](https://cranelift.dev/), placing Forge alongside Node.js/V8 in recursive benchmarks.

<details>
<summary>Full cross-language benchmark (fib(30))</summary>

| Language                | Time     | Relative |
| ----------------------- | -------- | -------- |
| Rust 1.91 (-O)          | 1.46ms   | baseline |
| C (clang -O2)           | 1.57ms   | ~1.1x    |
| Go 1.23                 | 4.24ms   | ~2.9x    |
| Scala 2.12 (JVM)        | 4.33ms   | ~3.0x    |
| Java 1.8 (JVM)          | 5.77ms   | ~4.0x    |
| JavaScript (Node 22/V8) | 9.53ms   | ~6.5x    |
| **Forge (JIT)**         | **10ms** | **~7x**  |
| Python 3                | 114ms    | ~79x     |
| Forge (VM)              | 252ms    | ~173x    |
| Forge (interpreter)     | 2,300ms  | ~1,575x  |

</details>

---

## Standard Library

15 modules. No imports needed.

### HTTP Server

```forge
@server(port: 3000)

@get("/users/:id")
fn get_user(id: String) -> Json {
    return db.query("SELECT * FROM users WHERE id = " + id)
}

@post("/users")
fn create_user(body: Json) -> Json {
    db.execute("INSERT INTO users (name) VALUES (\"" + body.name + "\")")
    return { created: true }
}
```

### HTTP Client

```forge
let resp = fetch("https://api.github.com/repos/rust-lang/rust")
say resp.json.stargazers_count

let data = http.post("https://httpbin.org/post", { name: "Forge" })
say data.status
```

### Database (SQLite + PostgreSQL)

```forge
db.open(":memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES (\"Alice\")")
let users = db.query("SELECT * FROM users")
term.table(users)
db.close()
```

### Shell Integration

```forge
say sh("whoami")                           // quick stdout
let files = sh_lines("ls /etc | head -5")  // stdout as array
if sh_ok("which docker") { say "Docker installed" }
let path = which("git")                    // find command path
let sorted = pipe_to(csv_data, "sort")     // pipe Forge data into commands
say cwd()                                  // working directory
```

### Crypto

```forge
say crypto.sha256("forge")
say crypto.base64_encode("secret")
say crypto.md5("data")
```

### File System

```forge
fs.write("config.json", json.stringify(data))
let content = fs.read("config.json")
say fs.exists("config.json")
say fs.list(".")
```

### Terminal UI

```forge
term.table(data)                   // formatted tables
term.sparkline([1, 5, 3, 8, 2])   // inline charts
term.bar("Progress", 75, 100)     // progress bars
say term.red("Error!")             // colored output
term.banner("FORGE")               // ASCII art
term.success("All tests passed!") // status messages
```

### All Modules

| Module   | What's In It                                                                   |
| -------- | ------------------------------------------------------------------------------ |
| `math`   | sqrt, pow, abs, sin, cos, tan, pi, e, random, floor, ceil, round               |
| `fs`     | read, write, append, exists, list, mkdir, copy, rename, remove, size           |
| `crypto` | sha256, md5, base64_encode/decode, hex_encode/decode                           |
| `db`     | SQLite — open, query, execute, close                                           |
| `pg`     | PostgreSQL — connect, query, execute, close                                    |
| `json`   | parse, stringify, pretty                                                       |
| `csv`    | parse, stringify, read, write                                                  |
| `regex`  | test, find, find_all, replace, split                                           |
| `env`    | get, set, has, keys                                                            |
| `log`    | info, warn, error, debug                                                       |
| `term`   | colors, table, sparkline, bar, banner, box, gradient, countdown, confirm, menu |
| `http`   | get, post, put, delete, patch, head, download, crawl                           |
| `io`     | prompt, print                                                                  |

---

## Object Helpers & Method Chaining

```forge
let user = { name: "Alice", age: 30, password: "secret" }

// Safe access with defaults
say get(user, "email", "N/A")                        // N/A
say get(resp, "json.user.profile.name", "unknown")   // deep dot-path, never crashes

// Transform objects
let public = pick(user, ["name", "age"])          // extract fields
let cleaned = omit(user, ["password"])             // remove fields
let config = merge({ port: 3000 }, { port: 8080 }) // merge (later wins)

// Search arrays
let admin = users.find(fn(u) { return u.role == "admin" })

// Chain operations
let names = users
    .filter(fn(u) { u.active })
    .map(fn(u) { u.name })
say names

// Check keys
say has_key(user, "email")
say contains(user, "name")
```

---

## CLI

| Command               | What It Does          |
| --------------------- | --------------------- |
| `forge run <file>`    | Run a program         |
| `forge`               | Start REPL            |
| `forge -e '<code>'`   | Evaluate inline       |
| `forge learn [n]`     | Interactive tutorials |
| `forge new <name>`    | Scaffold a project    |
| `forge test [dir]`    | Run tests             |
| `forge fmt [files]`   | Format code           |
| `forge build <file>`  | Compile to bytecode   |
| `forge install <src>` | Install a package     |
| `forge lsp`           | Language server       |
| `forge chat`          | AI assistant          |
| `forge version`       | Version info          |

---

## Examples

```bash
forge run examples/hello.fg        # basics
forge run examples/natural.fg      # natural syntax
forge run examples/api.fg          # REST API server
forge run examples/data.fg         # data processing + visualization
forge run examples/devops.fg       # system automation
forge run examples/showcase.fg     # everything in one file
forge run examples/functional.fg   # closures, recursion, higher-order
forge run examples/adt.fg          # algebraic data types + matching
forge run examples/result_try.fg   # error handling with ?
```

See [examples/](examples/) for the full list.

---

## Architecture

```
Source (.fg) → Lexer → Tokens → Parser → AST → Type Checker
                                                     ↓
                            ┌────────────────────────┼────────────────────────┐
                            ↓                        ↓                        ↓
                       Interpreter              Bytecode VM              JIT Compiler
                     (full features)           (--vm flag)             (--jit flag)
                            ↓                        ↓                        ↓
                     Runtime Bridge            Mark-Sweep GC          Cranelift Native
                  (axum, reqwest, tokio,       Green Threads              Code
                   rusqlite, postgres)
```

16,000+ lines of Rust. Zero `unsafe` blocks in application code. Built on:

| Crate                                              | Purpose         |
| -------------------------------------------------- | --------------- |
| [axum](https://github.com/tokio-rs/axum)           | HTTP server     |
| [tokio](https://tokio.rs)                          | Async runtime   |
| [reqwest](https://github.com/seanmonstar/reqwest)  | HTTP client     |
| [cranelift](https://cranelift.dev/)                | JIT compilation |
| [rusqlite](https://github.com/rusqlite/rusqlite)   | SQLite          |
| [ariadne](https://github.com/zesterer/ariadne)     | Error reporting |
| [rustyline](https://github.com/kkawakam/rustyline) | REPL            |
| [clap](https://github.com/clap-rs/clap)            | CLI             |

---

## Editor Support

### VS Code

Syntax highlighting is available in [editors/vscode/](editors/vscode/). To install locally:

```bash
cp -r editors/vscode ~/.vscode/extensions/forge-lang
```

### LSP

Forge ships with a built-in language server:

```bash
forge lsp
```

Configure your editor's LSP client to use `forge lsp` as the command.

---

## Project Status

Forge is v0.2.0. The language, interpreter, and standard library are stable and tested. The bytecode VM and JIT compiler are available via `--vm` and `--jit` flags.

| Metric                   | Value               |
| ------------------------ | ------------------- |
| Lines of Rust            | ~16,000             |
| Standard library modules | 15                  |
| Built-in functions       | 160+                |
| Keywords                 | 80+                 |
| Tests                    | 287 Rust + 25 Forge |
| Interactive lessons      | 14                  |
| Example programs         | 12                  |
| Dependencies (CVEs)      | 280 crates (0 CVEs) |

---

## Known Limitations (v0.2.0)

Forge is a young language. These are documented, not hidden:

- **No parameterized SQL queries** — use string concatenation for now. Be cautious with user input.
- **Interpreter performance** is ~20x slower than Python for deep recursion. Use `--jit` for compute-heavy workloads (11x faster than Python) or `--vm` for general bytecode execution.
- **VM/JIT feature gap** — the JIT and VM support fewer features than the interpreter. Use the default interpreter for full stdlib, HTTP, and database access.
- **`regex` functions** take `(text, pattern)` argument order, not `(pattern, text)`.

See [ROADMAP.md](ROADMAP.md) for what's coming next.

---

## Roadmap

| Version | Focus                                            |
| ------- | ------------------------------------------------ |
| v0.3    | Parameterized SQL, package registry, async/await |
| v0.4    | Debugger, WASM target, expanded JIT coverage     |
| v0.5    | LSP completions, type inference, formatter v2    |
| v1.0    | Stable API, backwards compatibility guarantee    |

See [ROADMAP.md](ROADMAP.md) for the full plan. Have ideas? [Open an issue](https://github.com/humancto/forge-lang/issues).

---

## Contributing

```bash
git clone https://github.com/humancto/forge-lang.git
cd forge-lang
cargo build && cargo test
forge run examples/showcase.fg
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the architecture guide, how to add features, and PR guidelines.

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community standards.

---

## Community

- [Issues](https://github.com/humancto/forge-lang/issues) — Bug reports and feature requests
- [Discussions](https://github.com/humancto/forge-lang/discussions) — Questions, ideas, show & tell
- [RFCs](rfcs/) — Language design proposals

---

## Security

To report a security vulnerability, please email the maintainers directly instead of opening a public issue. See [SECURITY.md](SECURITY.md) for details.

---

## License

[MIT](LICENSE)

---

<p align="center"><em>Stop installing. Start building.</em></p>
