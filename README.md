<div align="center">

# ⚒️ Forge

### The internet-native programming language that reads like English.

Built-in HTTP, databases, crypto, AI, and a JIT compiler.<br>
**22 stdlib modules. 200+ functions. No extra packages required.**

[![CI](https://github.com/humancto/forge-lang/actions/workflows/ci.yml/badge.svg)](https://github.com/humancto/forge-lang/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/humancto/forge-lang?color=%23ff6b35&style=flat-square)](https://github.com/humancto/forge-lang/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-64ffda?style=flat-square)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built_with-Rust-%23f74c00?style=flat-square)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-2,200%2B_passing-64ffda?style=flat-square)](#project-status)
[![Stars](https://img.shields.io/github/stars/humancto/forge-lang?color=%23ff6b35&style=flat-square)](https://github.com/humancto/forge-lang/stargazers)
[![crates.io](https://img.shields.io/crates/v/forge-lang?color=%23ff6b35&style=flat-square)](https://crates.io/crates/forge-lang)

[📥 **Download Book**](https://github.com/humancto/forge-lang/releases/download/v0.4.1/programming-forge.pdf) · [🤖 **llms.txt**](llms.txt) · [📖 **Language Spec**](https://humancto.github.io/forge-lang/spec/) · [🌐 **Website**](https://humancto.github.io/forge-lang/) · [💬 **Discussions**](https://github.com/humancto/forge-lang/discussions) · [🐛 **Issues**](https://github.com/humancto/forge-lang/issues)

</div>

---

```bash
brew install humancto/tap/forge    # install
forge learn                        # 30 interactive tutorials
forge run app.fg                   # run a program
```

---

## ⚡ See It In Action

<table>
<tr>
<td width="50%">

**REST API — 3 lines, zero deps**

```forge
@server(port: 3000)

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}
```

</td>
<td width="50%">

**Database + Crypto — built in**

```forge
db.open(":memory:")
db.execute("CREATE TABLE users (name TEXT)")
db.execute("INSERT INTO users VALUES ('Alice')")

let users = db.query("SELECT * FROM users")
term.table(users)

say crypto.sha256("password")
```

</td>
</tr>
</table>

> **No framework. No packages. No setup.** Just `forge run`.

---

## 📖 Table of Contents

|                                                      |                                 |                                       |
| ---------------------------------------------------- | ------------------------------- | ------------------------------------- |
| [⚡ Quick Example](#-see-it-in-action)               | [🎯 Why Forge?](#-why-forge)    | [📦 Installation](#-installation)     |
| [🗣️ Dual Syntax](#️-dual-syntax)                      | [🚀 Quick Tour](#-quick-tour)   | [🏗️ Type System](#️-type-system)       |
| [📚 Standard Library](#-standard-library-22-modules) | [⚡ Performance](#-performance) | [🎮 GenZ Debug Kit](#-genz-debug-kit) |
| [🔧 CLI](#-cli-commands)                             | [📂 Examples](#-examples)       | [🏛️ Architecture](#️-architecture)     |
| [📕 Book](#-the-book)                                | [🗺️ Roadmap](#️-roadmap)         | [🤝 Contributing](#-contributing)     |
| [🤖 AI agents (MCP)](#-use-forge-from-an-ai-agent-mcp) |                                 |                                       |

---

## 🎯 Why Forge?

Modern development means installing dozens of packages before writing a single line of logic:

```bash
# Python: pip install flask requests sqlalchemy bcrypt python-dotenv ...
# Node:   npm install express node-fetch better-sqlite3 bcrypt csv-parser ...
# Go:     go get github.com/gin-gonic/gin github.com/mattn/go-sqlite3 ...
```

**Forge: everything is built in.**

| Task               |    Forge    |    Python     |      Node.js       |        Go        |
| ------------------ | :---------: | :-----------: | :----------------: | :--------------: |
| REST API server    | **3 lines** |  12 + flask   |    15 + express    |     25 lines     |
| Query SQLite       | **2 lines** |    5 lines    | 8 + better-sqlite3 | 12 + go-sqlite3  |
| SHA-256 hash       | **1 line**  |    3 lines    |      3 lines       |     5 lines      |
| HTTP GET request   | **1 line**  | 3 + requests  |   5 + node-fetch   |     10 lines     |
| Parse CSV          | **1 line**  |    4 lines    |   6 + csv-parser   |     8 lines      |
| Terminal table     | **1 line**  | 5 + tabulate  |   4 + cli-table    | 10 + tablewriter |
| Retry with backoff | **1 line**  | 12 + tenacity |  15 + async-retry  |  20 + retry-go   |

> 💡 **Forge ships with 0 external dependencies needed** for HTTP, databases, crypto, CSV, regex, terminal UI, shell integration, AI, and more.

---

## 📦 Installation

```bash
# Homebrew (macOS & Linux) — recommended
brew install humancto/tap/forge

# Cargo (Rust)
cargo install forge-lang

# Install script
curl -fsSL https://raw.githubusercontent.com/humancto/forge-lang/main/install.sh | bash

# From source
git clone https://github.com/humancto/forge-lang.git && cd forge-lang && cargo install --path .
```

**Verify:**

```bash
forge version          # → Forge v0.9.0
forge learn            # 30 interactive tutorials
forge                  # start REPL
```

---

## 🗣️ Dual Syntax

Write it your way. Both compile identically.

<table>
<tr>
<td width="50%">

**✦ Natural — reads like English**

```forge
set name to "Forge"
say "Hello, {name}!"

define greet(who) {
    say "Welcome, {who}!"
}

set mut score to 0
repeat 5 times {
    change score to score + 10
}

if score > 40 {
    yell "You win!"
} otherwise {
    whisper "try again..."
}
```

</td>
<td width="50%">

**⚙ Classic — familiar syntax**

```forge
let name = "Forge"
println("Hello, {name}!")

fn greet(who) {
    println("Welcome, {who}!")
}

let mut score = 0
for i in range(0, 5) {
    score += 10
}

if score > 40 {
    println("You win!")
} else {
    println("try again...")
}
```

</td>
</tr>
</table>

<details>
<summary><strong>📋 Full syntax mapping (click to expand)</strong></summary>

| Concept    | Classic                    | Natural                     |
| ---------- | -------------------------- | --------------------------- |
| Variables  | `let x = 5`                | `set x to 5`                |
| Mutable    | `let mut x = 0`            | `set mut x to 0`            |
| Reassign   | `x = 10`                   | `change x to 10`            |
| Functions  | `fn add(a, b) { }`         | `define add(a, b) { }`      |
| Output     | `println("hi")`            | `say` / `yell` / `whisper`  |
| Else       | `else { }`                 | `otherwise { }` / `nah { }` |
| Async      | `async fn x() { }`         | `forge x() { }`             |
| Await      | `await expr`               | `hold expr`                 |
| Structs    | `struct Foo { }`           | `thing Foo { }`             |
| Methods    | `impl Foo { }`             | `give Foo { }`              |
| Interfaces | `interface Bar { }`        | `power Bar { }`             |
| Construct  | `Foo { x: 1 }`             | `craft Foo { x: 1 }`        |
| Fetch      | `fetch("url")`             | `grab resp from "url"`      |
| Loops      | `for i in range(0, 3) { }` | `repeat 3 times { }`        |
| Destruct   | `let {a, b} = obj`         | `unpack {a, b} from obj`    |

</details>

---

## 🚀 Quick Tour

### Variables & Functions

```forge
let name = "Forge"              // immutable
let mut count = 0               // mutable
count += 1

fn add(a, b) { return a + b }
let double = fn(x) { return x * 2 }   // anonymous function
```

### 🎤 The Output Trio

```forge
say "Normal volume"              // standard output
yell "LOUD AND PROUD!"          // UPPERCASE + !
whisper "quiet and gentle"       // lowercase + ...
```

### Control Flow & Pattern Matching

```forge
if score > 90 { say "A" }
otherwise if score > 80 { say "B" }
otherwise { say "C" }

// When guards
let label = when temp {
    > 100 -> "Boiling",
    > 60  -> "Warm",
    else  -> "Cold"
}

// Algebraic data types + pattern matching
type Shape = Circle(Float) | Rect(Float, Float)
match Circle(5.0) {
    Circle(r) => say "Area = {3.14 * r * r}"
    Rect(w, h) => say "Area = {w * h}"
}
```

### 🔑 Innovation Keywords

```forge
safe { risky_function() }                         // returns null on error
must parse_config("app.toml")                     // crash with clear message
check email is not empty                          // declarative validation
retry 3 times { fetch("https://api.example.com") } // automatic retry
timeout 5 seconds { long_operation() }             // enforced time limit
wait 2 seconds                                     // sleep with units
```

### Collections & Functional

```forge
let nums = [1, 2, 3, 4, 5]
let result = nums.filter(fn(x) { return x % 2 == 0 }).map(fn(x) { return x * 2 })
say result   // [4, 8]

let user = { name: "Alice", age: 30 }
say pick(user, ["name"])            // { name: "Alice" }
say get(user, "email", "N/A")      // safe access with default
```

### Error Handling

```forge
fn safe_divide(a, b) {
    if b == 0 { return Err("division by zero") }
    return Ok(a / b)
}

// Propagate with ?
fn halve_quotient(a, b) {
    let q = safe_divide(a, b)?
    return Ok(q / 2)
}

if is_err(halve_quotient(10, 0)) { say "failed" }
say unwrap_or(halve_quotient(20, 2), 0)   // 5
```

---

## 🏗️ Type System

Define data types, attach behavior, enforce contracts, and compose with delegation.

<table>
<tr>
<td width="50%">

**✦ Natural — thing / give / power**

```forge
thing Person {
    name: String,
    age: Int,
    role: String = "member"
}

give Person {
    define greet(it) {
        return "Hi, I'm " + it.name
    }
}

power Describable {
    fn describe() -> String
}

give Person the power Describable {
    define describe(it) {
        return it.name + " (" + it.role + ")"
    }
}

set p to craft Person { name: "Alice", age: 30 }
say p.greet()                      // Hi, I'm Alice
say satisfies(p, Describable)      // true
```

</td>
<td width="50%">

**⚙ Classic — struct / impl / interface**

```forge
struct Person {
    name: String,
    age: Int,
    role: String = "member"
}

impl Person {
    fn greet(it) {
        return "Hi, I'm " + it.name
    }
}

interface Describable {
    fn describe() -> String
}

impl Describable for Person {
    fn describe(it) {
        return it.name + " (" + it.role + ")"
    }
}

let p = Person { name: "Alice", age: 30 }
println(p.greet())                 // Hi, I'm Alice
println(satisfies(p, Describable)) // true
```

</td>
</tr>
</table>

### Composition with `has`

```forge
thing Address { street: String, city: String }
thing Employee { name: String, has addr: Address }

give Address {
    define full(it) { return it.street + ", " + it.city }
}

set emp to craft Employee {
    name: "Charlie",
    addr: craft Address { street: "123 Main St", city: "Portland" }
}

say emp.city        // delegated to emp.addr.city → "Portland"
say emp.full()      // delegated to emp.addr.full() → "123 Main St, Portland"
```

| Keyword                 | Purpose                                     |
| ----------------------- | ------------------------------------------- |
| `thing` / `struct`      | Define a data type                          |
| `craft`                 | Construct an instance                       |
| `give` / `impl`         | Attach methods to a type                    |
| `power` / `interface`   | Define a behavioral contract                |
| `has`                   | Embed a type with field + method delegation |
| `it`                    | Self-reference in methods                   |
| `satisfies(obj, Power)` | Check if an object satisfies a contract     |

---

## 📚 Standard Library (22 Modules)

Every module is available from line 1. No imports. No installs.

### 🌐 HTTP Server & Client

```forge
// Server — 3 lines to production
@server(port: 3000)
@get("/users/:id")
fn get_user(id: String) -> Json {
    return db.query("SELECT * FROM users WHERE id = ?", [id])
}

// Client — just fetch
let resp = fetch("https://api.github.com/repos/rust-lang/rust")
say resp.json.stargazers_count
```

### 🗄️ Database (SQLite + PostgreSQL + MySQL)

```forge
db.open(":memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice')")
let users = db.query("SELECT * FROM users")
term.table(users)

// MySQL — parameterized queries, connection pooling
let conn = mysql.connect("mysql://root:pass@localhost/mydb")
let users = mysql.query(conn, "SELECT * FROM users WHERE age > ?", [21])
mysql.close(conn)
```

### 🔐 JWT Authentication

```forge
let token = jwt.sign({ user_id: 123, role: "admin" }, "secret", { expires: "1h" })
let claims = jwt.verify(token, "secret")
say claims.user_id       // 123
say jwt.valid(token, "secret")  // true
```

### 🐚 Shell Integration

```forge
say sh("whoami")                           // quick stdout
let files = sh_lines("ls /etc | head -5")  // stdout as array
if sh_ok("which docker") { say "Docker installed" }
let sorted = pipe_to(csv_data, "sort")     // pipe Forge data into shell
```

Shell builtins are opt-in: run scripts with `forge --allow-run run script.fg`.

### 🖥️ Terminal UI

```forge
term.table(data)                   // formatted tables
term.sparkline([1, 5, 3, 8, 2])   // inline charts ▁▅▃█▂
term.bar("Progress", 75, 100)     // progress bars
say term.red("Error!")             // 🔴 colored output
say term.green("Success!")         // 🟢
term.banner("FORGE")               // ASCII art banner
```

### 🔐 Crypto + 📄 CSV + 📁 File System

```forge
say crypto.sha256("forge")                    // hash anything
say crypto.base64_encode("secret")             // encode/decode

let data = csv.read("users.csv")               // parse CSV files
csv.write("output.csv", processed_data)         // write CSV

fs.write("config.json", json.stringify(data))   // file I/O
let exists = fs.exists("config.json")
```

<details>
<summary><strong>📋 All 22 modules at a glance (click to expand)</strong></summary>

| Module     | Functions                                                                                                                            |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| **math**   | sqrt, pow, abs, sin, cos, tan, pi, e, random, random_int, clamp, floor, ceil, round                                                  |
| **fs**     | read, write, append, exists, list, mkdir, copy, rename, remove, size, lines, dirname, basename, join_path, is_dir, is_file, temp_dir |
| **crypto** | sha256, md5, base64_encode/decode, hex_encode/decode                                                                                 |
| **db**     | SQLite — open, query, execute, close (parameterized queries supported)                                                               |
| **pg**     | PostgreSQL — connect, query, execute, close (parameterized queries supported)                                                        |
| **mysql**  | MySQL — connect, query, execute, close (parameterized queries, connection pooling)                                                   |
| **jwt**    | sign, verify, decode, valid (HS256/384/512, RS256, ES256)                                                                            |
| **json**   | parse, stringify, pretty                                                                                                             |
| **csv**    | parse, stringify, read, write                                                                                                        |
| **regex**  | test, find, find_all, replace, split                                                                                                 |
| **env**    | get, set, has, keys                                                                                                                  |
| **log**    | info, warn, error, debug                                                                                                             |
| **term**   | colors, table, sparkline, bar, banner, box, gradient, countdown, confirm, menu                                                       |
| **http**   | get, post, put, delete, patch, head, download, crawl                                                                                 |
| **io**     | prompt, print, args_parse, args_get, args_has                                                                                        |
| **time**   | now, format, parse, add, diff, sleep, elapsed, unix, zone, and more                                                                  |
| **os**     | hostname, platform, arch, pid, cpus, homedir                                                                                         |
| **path**   | join, resolve, relative, is_absolute, dirname, basename, extname, separator                                                          |
| **url**    | parse, build, encode, decode                                                                                                         |
| **toml**   | parse, stringify, read                                                                                                               |
| **ws**     | WebSocket client — connect, send, receive, close                                                                                     |
| **npc**    | Fake data — name, email, username, phone, number, pick, bool, sentence, id, color, ip, url, company                                  |

</details>

---

## ⚡ Performance

Three execution tiers — pick your tradeoff:

| Engine                 | Flag       | Best For                                                         |
| ---------------------- | ---------- | ---------------------------------------------------------------- |
| ⚙️ Bytecode VM         | (default)  | General programs                                                 |
| 🔥 VM + Cranelift JIT  | `--jit`    | Tight numeric leaf functions (Int/Float math, loops)             |
| 📦 Tree-walking interp | `--interp` | Full feature surface; HTTP servers fall back to it automatically |

Measured on one 4-vCPU x86_64 Linux VM (Intel Xeon @ 2.10GHz) with a release build, wall-clock including process startup. Numbers vary by machine — run them yourself.

| Workload                             | VM (default) | `--jit` | `--interp` | Python 3.11 |
| ------------------------------------ | -----------: | ------: | ---------: | ----------: |
| Recursive `fib(30)`                  |       ~14 ms |  ~14 ms |     ~1.3 s |     ~100 ms |
| Numeric `while` loop, 20M iterations |       ~30 ms |  ~27 ms |     ~7.9 s |     ~0.9 s  |
| Startup (`forge -e 'println(1)'`)    |        ~6 ms |       — |          — |           — |

The JIT compiles functions over `Int`/`Bool` values (arithmetic, comparisons, loops, self-recursion) to native code, guarded by type checks that fall back to the VM, so it never changes results. The default VM tiers a function up after 100 calls, or after 1000 iterations of a loop inside it, so both rows above run native by default. Code the JIT does not accept (floats, strings, collections, closures) runs in the VM; `tools/bench_vm.sh` and `tools/bench_interp.sh` cover those workloads.

<details>
<summary><strong>🌐 HTTP Server benchmark — 20,000 requests / 200 concurrent (GET /ping → JSON)</strong></summary>

| Server                         |    Req/sec | Avg Latency |    vs Python    |
| ------------------------------ | ---------: | ----------: | :-------------: |
| **Forge** (axum + interpreter) | **28,017** |      7.1 ms | **9.8x faster** |
| Rust / Axum (native async)     |     24,853 |      8.0 ms |   8.7x faster   |
| Python / Flask (threaded)      |      2,854 |     70.0 ms |      1.0x       |

Forge's HTTP server is built on axum + tokio — the same stack powering production Rust services. For typical JSON API endpoints, Forge matches raw Rust throughput while giving you a 4-line handler instead of 40.

Measured at v0.4 with ApacheBench (`ab -n 20000 -c 200`) on localhost, macOS. The server has since moved to a per-request fork model, so re-measure on your hardware:

```bash
# Terminal 1
forge run examples/bench_server.fg

# Terminal 2
forge run examples/bench_client.fg     # Forge-native client with stats
ab -n 20000 -c 200 http://127.0.0.1:9090/ping   # or use ab/wrk/oha
```

</details>

---

## 🎮 GenZ Debug Kit

Debugging should have personality. Forge ships both classic and GenZ-flavored debug tools.

```forge
sus(my_object)         // 🔍 inspect any value (= debug/inspect)
bruh("it broke")       // 💀 crash with a message (= panic)
bet(score > 0)         // 💯 assert it's true (= assert)
no_cap(result, 42)     // 🧢 assert equality (= assert_eq)
ick(is_deleted)        // 🤮 assert it's false (= assert_false)
```

**Plus execution tools:**

```forge
cook { expensive_op() }           // ⏱️ profile execution time
slay 1000 { fibonacci(20) }      // 📊 benchmark (1000 iterations)
ghost { might_fail() }           // 👻 silent execution (swallow errors)
yolo { send_analytics(data) }    // 🚀 fire-and-forget async
```

---

## 🔧 CLI Commands

| Command                       | What It Does                                         |
| ----------------------------- | ---------------------------------------------------- |
| `forge run <file>`            | Run a `.fg` program or `.fgc` bytecode               |
| `forge` / `forge repl`        | Start REPL                                           |
| `forge -e '<code>'`           | Evaluate inline                                      |
| `forge learn [n]`             | 30 interactive tutorials                             |
| `forge new <name>`            | Scaffold a project                                   |
| `forge test [dir]`            | Run `@test` functions (`--coverage`, `--filter`)     |
| `forge fmt [files]`           | Format code (`--check` for CI)                       |
| `forge build <file>`          | Compile to `.fgc` bytecode                           |
| `forge build --native <file>` | Native executable embedding source (servers work)    |
| `forge build --aot <file>`    | Native executable embedding bytecode (VM programs)   |
| `forge install` / `add` / `update` / `search` / `publish` | Package management       |
| `forge watch <file>`          | Re-run on file changes                               |
| `forge doc [paths]`           | Generate documentation                               |
| `forge lsp` / `forge dap`     | Language server / debug adapter                      |
| `forge mcp`                   | MCP server: AI agents run Forge in a sandbox         |
| `forge chat`                  | AI assistant                                         |
| `forge version`               | Version info                                         |

**Global flags go before the subcommand:** `forge --interp run app.fg`, `forge --jit run app.fg`, `forge --allow-run run deploy.fg`. Also `--profile` and `--strict`. `--vm` is accepted for compatibility and does nothing (the VM is already the default).

Native builds are standalone when `libforge_lang.a` is available (set `FORGE_LIB_DIR`, or keep it next to the `forge` binary); otherwise Forge builds a launcher that runs the program through an installed `forge`.

---

## 🤖 Use Forge from an AI agent (MCP)

`forge mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server over stdio. It gives an agent a sandboxed Forge runtime ("code mode"): instead of many tool calls, the agent writes one short script — fetch, filter, compute, print — and runs it.

| Tool              | What it does                                                                                                    |
| ----------------- | --------------------------------------------------------------------------------------------------------------- |
| `run_forge`       | Runs `{code, timeout_secs?, max_fuel?}` in the sandbox; returns what the script printed, or `isError` with a typed error (`syntax`, `permission_denied`, `runtime`, `timeout`, `output_limit`, `fuel_exhausted`, `memory_limit`, `resource_limit`) and the output so far |
| `check_forge`     | Parses and type-checks `{code}` without running it; returns diagnostics with line numbers                      |
| `forge_reference` | The compact language guide ([`llms.txt`](llms.txt)) so the agent can learn Forge                                |

Scripts are **denied everything by default** — files, network, environment, databases, subprocesses, AI calls, `exit()`. Grant only what the agent needs with the usual flags (or `[permissions]` in a `forge.toml` in the server's working directory; flags win):

```bash
forge mcp                                         # pure computation only
forge mcp --allow-net=api.example.com             # HTTP to one host
forge mcp --allow-read=./data --allow-write=./out --max-time 10
forge mcp --max-fuel 50000000 --max-memory 128MB  # tighter per-call resource limits
```

Claude Desktop (`claude_desktop_config.json`) or a project `.mcp.json` for Claude Code:

```json
{
  "mcpServers": {
    "forge": { "command": "forge", "args": ["mcp", "--allow-net=api.example.com"] }
  }
}
```

or `claude mcp add forge -- forge mcp --allow-net=api.example.com`.

- `--max-time` (default 30s) is the per-call limit; an agent's `timeout_secs` can only lower it. Output returned to the agent is capped at 64 KiB (a script printing over 1 MiB is stopped).
- Each call also runs under deterministic resource limits: 200M steps of fuel (`--max-fuel`; an agent's `max_fuel` can only lower it), 256 MiB of memory (`--max-memory`), and caps on open files, sockets, subprocesses, tasks, value sizes and imports. A runaway loop fails with `fuel exhausted` at the same step every time; the server keeps serving. Details: [SECURITY.md — Resource limits](SECURITY.md#resource-limits).
- Each call runs on its own thread: a stuck script times out while the server keeps answering, and `notifications/cancelled` stops it. `run` (shell) is never granted unless you pass `--allow-run`.
- Nothing a script prints or reads can reach the protocol stream (stdin/stdout are moved off fds 0/1 on Unix).
- Protocol: `2026-07-28` (stateless, `server/discover`) and the `initialize` handshake for `2025-11-25` back to `2024-11-05`. Rust hosts can embed the same server: `forge_lang::mcp::serve(reader, writer, config)`.

---

## 📂 Examples

```bash
forge run examples/hello.fg        # basics
forge run examples/natural.fg      # natural syntax
forge run examples/types.fg        # type system — thing/give/power/craft/has
forge run examples/api.fg          # REST API server
forge run examples/data.fg         # data processing + visualization
forge run examples/devops.fg       # system automation
forge run examples/showcase.fg     # everything in one file
forge run examples/functional.fg   # closures, recursion, higher-order
forge run examples/adt.fg          # algebraic data types + matching
forge run examples/result_try.fg   # error handling with ?
forge run examples/jwt_demo.fg     # JWT authentication
forge run examples/mysql_demo.fg   # MySQL database CRUD
```

---

## 🏛️ Architecture

```
Source (.fg) → Lexer → Tokens → Parser → AST → Type Checker
                                                     ↓
                            ┌────────────────────────┼────────────────────────┐
                            ↓                        ↓                        ↓
                       Interpreter              Bytecode VM              JIT Compiler
                     (--interp flag)            (default)              (--jit flag)
                            ↓                        ↓                        ↓
                     Runtime Bridge            Mark-Sweep GC          Cranelift Native
                  (axum, reqwest, tokio,       Green Threads              Code
                   rusqlite, postgres)
```

**60k+ lines of Rust.** `unsafe` is confined to the C ABI entry points used by native binaries (`src/lib.rs`) and the JIT's native-code boundary (`src/vm/`).

<details>
<summary><strong>🔩 Core dependencies</strong></summary>

| Crate                                              | Purpose         |
| -------------------------------------------------- | --------------- |
| [axum](https://github.com/tokio-rs/axum)           | HTTP server     |
| [tokio](https://tokio.rs)                          | Async runtime   |
| [reqwest](https://github.com/seanmonstar/reqwest)  | HTTP client     |
| [cranelift](https://cranelift.dev/)                | JIT compilation |
| [rusqlite](https://github.com/rusqlite/rusqlite)   | SQLite          |
| [ariadne](https://github.com/zesterer/ariadne)     | Error reporting |
| [rustyline](https://github.com/kkawakam/rustyline) | REPL            |
| [clap](https://github.com/clap-rs/clap)            | CLI parsing     |

</details>

---

## 📕 The Book

<p align="center">
  <a href="https://github.com/humancto/forge-lang/releases/download/v0.4.1/programming-forge.pdf">
    <img src="docs/cover.jpeg" alt="Programming Forge — The Internet-Native Language That Reads Like English" width="280">
  </a>
</p>

<p align="center">
  <strong>Programming Forge: The Internet-Native Language That Reads Like English</strong><br>
  36 chapters · Foundations · Standard Library · Real-World Projects · Internals<br><br>
  <a href="https://github.com/humancto/forge-lang/releases/download/v0.4.1/programming-forge.pdf">📥 Download PDF (v0.4 edition)</a> · <a href="docs/PROGRAMMING_FORGE.md">📖 Read Online</a>
</p>

---

## 📊 Project Status

Forge is **v0.9.0**. The bytecode VM is the default engine and matches the reference tree-walking interpreter on the full test suite; a guarded JIT tier compiles hot integer code.

| Metric                   |                            Value |
| ------------------------ | -------------------------------: |
| Lines of Rust            |                             60k+ |
| Standard library modules |                               22 |
| Stdlib functions         |                             200+ |
| Tests passing            | 2,200+ (1,600+ Rust, 600+ Forge) |
| Interactive lessons      |                               30 |
| Example programs         |                              20+ |

### Known Limitations

> [!NOTE]
> Forge is a young language. These are documented, not hidden.

- **VM parity is in progress** — the default VM does not yet cover everything the interpreter does (for example implicit last-expression returns, `match` on `Ok`/`Err`, SQL bind parameters, and the `url`/`toml`/`npc`/`ws` modules). If a program behaves unexpectedly, try `forge --interp run`. Gaps are tracked in [ROADMAP.md](ROADMAP.md).
- **Parameterized SQL queries** — pass a params array as the second argument to `db.query`, `db.execute`, `pg.query`, `pg.execute`, `mysql.query`, and `mysql.execute` to bind user input safely.
- **Shell access is opt-in** — `sh`, `run_command` and friends need `forge --allow-run run ...`.
- **`regex` functions** take `(text, pattern)` argument order, not `(pattern, text)`.

---

## 🗺️ Roadmap

| Version         | Focus                                                                                 |
| --------------- | ------------------------------------------------------------------------------------- |
| **v0.3** ✅     | Type system (thing/power/give/craft/has), GenZ debug kit, NPC module                  |
| **v0.4** ✅     | JWT auth, MySQL, parameterized SQL (all DBs), CORS, PG TLS                            |
| **v0.5–0.7** ✅ | Packages + registry, LSP/DAP, VM as default engine, native/AOT builds                 |
| **v0.8** ✅     | `os`/`path` modules, `--allow-run`, SSRF guard, optional JIT/DB cargo features        |
| **v0.9** ✅     | Guarded JIT tier, full VM parity, VM/interpreter perf, sandbox (`--sandbox`, `--allow-*`), `forge mcp` |
| **Next**        | Sandbox memory limits, true OSR, Arc-shared values, broader JIT types (float/string)  |
| **v1.0**        | Stable API, compatibility guarantees, production hardening                            |

See [ROADMAP.md](ROADMAP.md) for the public roadmap. Have ideas? [Open an issue](https://github.com/humancto/forge-lang/issues).

---

## ✏️ Editor Support

**VS Code** — Syntax highlighting available in [editors/vscode/](editors/vscode/):

```bash
cp -r editors/vscode ~/.vscode/extensions/forge-lang
```

**LSP** — Built-in language server:

```bash
forge lsp
```

Configure your editor's LSP client to use `forge lsp` as the command.

---

## 🤝 Contributing

```bash
git clone https://github.com/humancto/forge-lang.git
cd forge-lang
cargo build && cargo test
forge run examples/showcase.fg
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the architecture guide and PR guidelines. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community standards.

---

## 🔒 Security

### Sandboxing and permissions

Forge has a Deno-style capability model shared by both engines. Defaults are unchanged (`forge run` allows everything except subprocesses), and `--sandbox` turns it into default-deny:

```bash
forge run --sandbox --allow-read=./data --allow-net=api.example.com agent.fg
forge run --max-time 10 job.fg     # wall-clock limit (exit 124)
forge run --max-fuel 50000000 --max-memory 256MB job.fg   # deterministic step budget + memory cap
```

Capabilities: `fs.read`, `fs.write` (path-scoped, symlink- and `..`-safe), `net` (host allowlist), `env`, `db`, `run`, `ai`. Denials read `permission denied: fs.write (/etc/passwd) — run with --allow-write or grant it in the host policy`. The same policy can go in `forge.toml` under `[permissions]`.

Embedding Forge in a Rust host (for AI agents and automation) starts from deny-all:

```rust
let out = forge_lang::Sandbox::new()
    .allow_read(["./data"])
    .max_time(std::time::Duration::from_secs(5))
    .run_source(r#"say "hi""#)?;
assert_eq!(out.stdout, "hi\n");
```

AI agents can use the same sandbox through [`forge mcp`](#-use-forge-from-an-ai-agent-mcp).

Details and current limits: [SECURITY.md — Sandboxing and permissions](SECURITY.md#sandboxing-and-permissions).

To report a security vulnerability, please email the maintainers directly instead of opening a public issue. See [SECURITY.md](SECURITY.md).

---

## 📄 License

[MIT](LICENSE)

---

<div align="center">

**Stop installing. Start building.**

Built by [**HumanCTO**](https://www.humancto.com) · [GitHub](https://github.com/humancto) · [LinkedIn](https://www.linkedin.com/in/archithr/)

</div>
