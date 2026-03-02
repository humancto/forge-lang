<div align="center">

# ⚒️ Forge

### The internet-native programming language that reads like English.

Built-in HTTP, databases, crypto, AI, and a JIT compiler.<br>
**16 modules. 230+ functions. Zero dependencies.**

[![CI](https://github.com/humancto/forge-lang/actions/workflows/ci.yml/badge.svg)](https://github.com/humancto/forge-lang/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/humancto/forge-lang?color=%23ff6b35&style=flat-square)](https://github.com/humancto/forge-lang/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-64ffda?style=flat-square)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built_with-Rust-%23f74c00?style=flat-square)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-822_passing-64ffda?style=flat-square)](#project-status)
[![Stars](https://img.shields.io/github/stars/humancto/forge-lang?color=%23ff6b35&style=flat-square)](https://github.com/humancto/forge-lang/stargazers)
[![crates.io](https://img.shields.io/crates/v/forge-lang?color=%23ff6b35&style=flat-square)](https://crates.io/crates/forge-lang)

[📥 **Download Book**](https://github.com/humancto/forge-lang/releases/latest/download/programming-forge.pdf) · [🌐 **Website**](https://humancto.github.io/forge-lang/) · [💬 **Discussions**](https://github.com/humancto/forge-lang/discussions) · [🐛 **Issues**](https://github.com/humancto/forge-lang/issues)

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
| [📚 Standard Library](#-standard-library-16-modules) | [⚡ Performance](#-performance) | [🎮 GenZ Debug Kit](#-genz-debug-kit) |
| [🔧 CLI](#-cli-commands)                             | [📂 Examples](#-examples)       | [🏛️ Architecture](#️-architecture)     |
| [📕 Book](#-the-book)                                | [🗺️ Roadmap](#️-roadmap)         | [🤝 Contributing](#-contributing)     |

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
forge version          # → forge 0.3.3
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
let double = fn(x) { x * 2 }   // lambda with implicit return
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
    > 100 -> "Boiling"
    > 60  -> "Warm"
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
let result = nums
    .filter(fn(x) { x % 2 == 0 })
    .map(fn(x) { x * 2 })
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

match safe_divide(10, 0) {
    Ok(val) => say "Got: {val}"
    Err(msg) => say "Error: {msg}"
}

// Propagate with ?
fn compute(input) {
    let n = parse_int(input)?
    return Ok(n * 2)
}
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

## 📚 Standard Library (16 Modules)

Every module is available from line 1. No imports. No installs.

### 🌐 HTTP Server & Client

```forge
// Server — 3 lines to production
@server(port: 3000)
@get("/users/:id")
fn get_user(id: String) -> Json {
    return db.query("SELECT * FROM users WHERE id = " + id)
}

// Client — just fetch
let resp = fetch("https://api.github.com/repos/rust-lang/rust")
say resp.json.stargazers_count
```

### 🗄️ Database (SQLite + PostgreSQL)

```forge
db.open(":memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice')")
let users = db.query("SELECT * FROM users")
term.table(users)
```

### 🐚 Shell Integration

```forge
say sh("whoami")                           // quick stdout
let files = sh_lines("ls /etc | head -5")  // stdout as array
if sh_ok("which docker") { say "Docker installed" }
let sorted = pipe_to(csv_data, "sort")     // pipe Forge data into shell
```

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
<summary><strong>📋 All 16 modules at a glance (click to expand)</strong></summary>

| Module     | Functions                                                                                                                            |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| **math**   | sqrt, pow, abs, sin, cos, tan, pi, e, random, random_int, clamp, floor, ceil, round                                                  |
| **fs**     | read, write, append, exists, list, mkdir, copy, rename, remove, size, lines, dirname, basename, join_path, is_dir, is_file, temp_dir |
| **crypto** | sha256, md5, base64_encode/decode, hex_encode/decode                                                                                 |
| **db**     | SQLite — open, query, execute, close                                                                                                 |
| **pg**     | PostgreSQL — connect, query, execute, close                                                                                          |
| **json**   | parse, stringify, pretty                                                                                                             |
| **csv**    | parse, stringify, read, write                                                                                                        |
| **regex**  | test, find, find_all, replace, split                                                                                                 |
| **env**    | get, set, has, keys                                                                                                                  |
| **log**    | info, warn, error, debug                                                                                                             |
| **term**   | colors, table, sparkline, bar, banner, box, gradient, countdown, confirm, menu                                                       |
| **http**   | get, post, put, delete, patch, head, download, crawl                                                                                 |
| **io**     | prompt, print, args_parse, args_get, args_has                                                                                        |
| **exec**   | run_command                                                                                                                          |
| **time**   | now, format, parse, sleep, elapsed                                                                                                   |
| **npc**    | Fake data — name, email, username, phone, number, pick, bool, sentence, id, color, ip, url, company                                  |

</details>

---

## ⚡ Performance

Three execution tiers — pick your tradeoff:

| Engine         |  fib(30) |   vs Python    | Best For                              |
| -------------- | -------: | :------------: | ------------------------------------- |
| 🔥 `--jit`     | **10ms** | **11x faster** | Compute-heavy hot functions           |
| ⚙️ `--vm`      |    252ms |   ~2x slower   | General bytecode execution            |
| 📦 Interpreter |  2,300ms |  ~20x slower   | Full feature set + all 230+ functions |

<details>
<summary><strong>📊 Full cross-language benchmark — fib(30)</strong></summary>

| Language                |     Time | Relative |
| ----------------------- | -------: | -------: |
| Rust 1.91 (-O)          |   1.46ms | baseline |
| C (clang -O2)           |   1.57ms |    ~1.1x |
| Go 1.23                 |   4.24ms |    ~2.9x |
| Scala 2.12 (JVM)        |   4.33ms |    ~3.0x |
| Java 1.8 (JVM)          |   5.77ms |    ~4.0x |
| JavaScript (Node 22/V8) |   9.53ms |    ~6.5x |
| **Forge (JIT)**         | **10ms** |  **~7x** |
| Python 3                |    114ms |     ~79x |
| Forge (VM)              |    252ms |    ~173x |
| Forge (interpreter)     |  2,300ms |  ~1,575x |

The JIT compiles hot functions to native code via [Cranelift](https://cranelift.dev/), placing Forge alongside Node.js/V8 for recursive workloads.

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

| Command               | What It Does             |
| --------------------- | ------------------------ |
| `forge run <file>`    | Run a program            |
| `forge`               | Start REPL               |
| `forge -e '<code>'`   | Evaluate inline          |
| `forge learn [n]`     | 30 interactive tutorials |
| `forge new <name>`    | Scaffold a project       |
| `forge test [dir]`    | Run tests                |
| `forge fmt [files]`   | Format code              |
| `forge build <file>`  | Compile to bytecode      |
| `forge install <src>` | Install a package        |
| `forge lsp`           | Language server          |
| `forge chat`          | AI assistant             |
| `forge version`       | Version info             |

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
```

---

## 🏛️ Architecture

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

**~26,000 lines of Rust.** Zero `unsafe` blocks in application code.

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
  <a href="https://github.com/humancto/forge-lang/releases/latest/download/programming-forge.pdf">
    <img src="docs/cover.jpeg" alt="Programming Forge — The Internet-Native Language That Reads Like English" width="280">
  </a>
</p>

<p align="center">
  <strong>Programming Forge: The Internet-Native Language That Reads Like English</strong><br>
  36 chapters · Foundations · Standard Library · Real-World Projects · Internals<br><br>
  <a href="https://github.com/humancto/forge-lang/releases/latest/download/programming-forge.pdf">📥 Download PDF (Free)</a> · <a href="docs/PROGRAMMING_FORGE.md">📖 Read Online</a>
</p>

---

## 📊 Project Status

Forge is **v0.3.3**. The language, interpreter, and standard library are stable and tested.

| Metric                   |                      Value |
| ------------------------ | -------------------------: |
| Lines of Rust            |                    ~26,000 |
| Standard library modules |                         16 |
| Built-in functions       |                       230+ |
| Keywords                 |                        80+ |
| Tests passing            | 822 (488 Rust + 334 Forge) |
| Interactive lessons      |                         30 |
| Example programs         |                         12 |
| Dependencies (CVEs)      |        280 crates (0 CVEs) |

### Known Limitations

> [!NOTE]
> Forge is a young language. These are documented, not hidden.

- **No parameterized SQL queries** — use string concatenation for now. Be cautious with user input.
- **Three execution tiers with different trade-offs** — The interpreter supports all 230+ functions. Use `--jit` for compute-heavy code (11x faster than Python) or `--vm` for bytecode execution.
- **VM/JIT feature gap** — The JIT and VM execute a subset of the language. Use the default interpreter for full stdlib, HTTP, database, and AI features.
- **`regex` functions** take `(text, pattern)` argument order, not `(pattern, text)`.

---

## 🗺️ Roadmap

| Version     | Focus                                                                                             |
| ----------- | ------------------------------------------------------------------------------------------------- |
| **v0.3** ✅ | Type system (thing/power/give/craft/has), 73 new functions, GenZ debug kit, NPC module, 822 tests |
| **v0.4**    | Parameterized SQL, package registry, debugger                                                     |
| **v0.5**    | WASM target, expanded JIT coverage, LSP completions                                               |
| **v1.0**    | Stable API, backwards compatibility guarantee                                                     |

See [ROADMAP.md](ROADMAP.md) for the full plan. Have ideas? [Open an issue](https://github.com/humancto/forge-lang/issues).

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

To report a security vulnerability, please email the maintainers directly instead of opening a public issue. See [SECURITY.md](SECURITY.md).

---

## 📄 License

[MIT](LICENSE)

---

<div align="center">

**Stop installing. Start building.**

Built by [**HumanCTO**](https://www.humancto.com) · [GitHub](https://github.com/humancto) · [LinkedIn](https://www.linkedin.com/in/archithr/)

</div>
