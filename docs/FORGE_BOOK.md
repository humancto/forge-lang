---
title: "Programming Forge"
subtitle: "The Internet-Native Language That Reads Like English"
author: "Archith Rapaka"
edition: "First Edition"
version: "0.3.0"
year: "2026"
publisher: "Self-Published"
cover_description: "A glowing anvil in a digital forge, sparks flying as molten code streams pour from it, forming HTTP requests, database queries, and terminal UI elements — all set against a dark navy background with subtle circuit-board patterns. The anvil sits on a workbench made of keyboard keys. The sparks form recognizable code symbols: curly braces, arrows, pipes. Color palette: deep navy (#0a192f), forge orange (#ff6b35), electric blue (#64ffda), white spark highlights."
---

# Programming Forge

**The Internet-Native Language That Reads Like English**

_By Archith Rapaka_

_First Edition — 2026_

---

## About This Book

This book is the definitive guide to the Forge programming language. It takes you from writing your first line of code to building production REST APIs, processing data, automating DevOps tasks, and understanding the internals of the language itself.

Forge was designed with a single premise: the things developers do most often — making HTTP requests, querying databases, handling JSON, hashing passwords — should be built into the language, not bolted on as libraries. You shouldn't need 47 dependencies to build a web server.

This book is organized in four parts:

- **Part I: Foundations** — Installation, syntax, and core language concepts
- **Part II: The Standard Library** — Every built-in module, function by function
- **Part III: Building Real Things** — HTTP servers, data pipelines, DevOps scripts, AI integration
- **Part IV: Under the Hood** — Architecture, the bytecode VM, and contributing to Forge

Whether you're a student writing your first program, a backend developer building APIs, or a systems programmer curious about language implementation — this book is for you.

---

# Part I: Foundations

---

## Chapter 1: Getting Started

### What Is Forge?

Forge is a programming language built for the internet age. It compiles to bytecode and runs on a register-based virtual machine written in Rust. But unlike most compiled languages, Forge was designed to feel approachable — its syntax reads like English, its standard library covers the full stack, and you can go from zero to a running REST API in under ten lines of code.

Here's what makes Forge different from the languages you already know:

| If you know... | Forge feels like...                                                   |
| -------------- | --------------------------------------------------------------------- |
| Python         | Same readability, but with real types and no GIL                      |
| JavaScript     | Same JSON-native objects, but no `undefined`, no `null` surprises     |
| Go             | Same simplicity and fast compilation, but with more expressive syntax |
| Rust           | Same safety guarantees, but without fighting the borrow checker       |
| Ruby           | Same developer happiness, but with HTTP and databases built in        |

### Installation

Forge is distributed as a single binary. You need Rust 1.85 or later to build it.

```
git clone https://github.com/forge-lang/forge.git
cd forge
cargo install --path .
```

Verify your installation:

```
$ forge version
Forge v0.3.0
Internet-native programming language
Bytecode VM with mark-sweep GC
```

### Your First Program

Create a file called `hello.fg`:

```
say "Hello, World!"
```

Run it:

```
$ forge run hello.fg
Hello, World!
```

That's it. No `main()` function, no imports, no boilerplate. Forge programs are sequences of statements executed from top to bottom.

### The REPL

Forge ships with an interactive read-eval-print loop. Start it by typing `forge` with no arguments:

```
$ forge

  ╔═══════════════════════════════════════╗
  ║  ⚒️  Forge REPL v0.3.0               ║
  ║  Type 'help' for commands             ║
  ╚═══════════════════════════════════════╝

forge> say "Hello!"
Hello!
forge> 2 + 2
4
forge> let name = "World"
forge> say "Hello, {name}!"
Hello, World!
```

The REPL supports multiline input (open a `{` and press Enter), command history (up/down arrows), and tab completion.

### Interactive Tutorials

If you prefer guided learning, Forge has 30 built-in interactive lessons:

```
$ forge learn
```

This walks you through the language step by step, from "Hello World" to databases and HTTP servers. Each lesson shows an explanation, a code example, the expected output, and lets you try it yourself.

To jump to a specific lesson:

```
$ forge learn 5
```

### Inline Evaluation

For quick one-liners, use the `-e` flag:

```
$ forge -e 'say math.sqrt(144)'
12

$ forge -e 'say crypto.sha256("hello")'
2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
```

---

## Chapter 2: Language Fundamentals

### Values and Types

Forge has seven fundamental types:

| Type     | Example                      | Description                             |
| -------- | ---------------------------- | --------------------------------------- |
| `Int`    | `42`, `-7`, `0`              | 64-bit signed integer                   |
| `Float`  | `3.14`, `-0.5`, `1.0`        | 64-bit floating point (IEEE 754)        |
| `String` | `"hello"`, `"Hi, {name}!"`   | UTF-8 string with interpolation         |
| `Bool`   | `true`, `false`              | Boolean                                 |
| `Array`  | `[1, 2, 3]`                  | Ordered, heterogeneous collection       |
| `Object` | `{ name: "Alice", age: 30 }` | Key-value map (insertion-ordered)       |
| `Null`   | `null`                       | Absence of value (rarely used directly) |

Forge also has three special value types:

| Type       | Example                  | Description                 |
| ---------- | ------------------------ | --------------------------- |
| `Function` | `fn(x) { return x * 2 }` | Named or anonymous function |
| `Result`   | `Ok(42)`, `Err("fail")`  | Success-or-failure wrapper  |
| `BuiltIn`  | `println`, `math.sqrt`   | Native Rust function        |

### Variables

Variables are declared with `let` (classic) or `set` (natural):

```
let name = "Forge"
set language to "Forge"
```

Both forms are equivalent. Use whichever reads better in context.

**Variables are immutable by default.** This is a deliberate design choice — immutability prevents an entire class of bugs. If you need to change a variable after creation, mark it as mutable:

```
let mut counter = 0
counter = counter + 1

set mut score to 0
change score to score + 10
```

The `change` keyword is the natural-syntax equivalent of reassignment.

### Type Annotations

Type annotations are optional. When present, they serve as documentation and are checked by the type checker:

```
let name: String = "Forge"
let count: Int = 42
let ratio: Float = 0.75
let active: Bool = true
```

Forge uses gradual typing — your code runs whether or not you add annotations, but the type checker will warn you about obvious mismatches.

### Strings and Interpolation

Strings in Forge use double quotes and support interpolation with curly braces:

```
let name = "World"
let greeting = "Hello, {name}!"
say greeting  // Hello, World!
```

Any expression can go inside the curly braces:

```
let x = 7
say "The answer is {x * 6}"  // The answer is 42
```

For strings that should not be interpolated, use triple quotes:

```
let raw = """This {is} not interpolated"""
say raw  // This {is} not interpolated
```

### Operators

#### Arithmetic

| Operator | Description    | Example                  |
| -------- | -------------- | ------------------------ |
| `+`      | Addition       | `3 + 4` → `7`            |
| `-`      | Subtraction    | `10 - 3` → `7`           |
| `*`      | Multiplication | `6 * 7` → `42`           |
| `/`      | Division       | `22 / 7` → `3` (integer) |
| `%`      | Modulo         | `17 % 5` → `2`           |

When mixing `Int` and `Float`, the result is promoted to `Float`:

```
say 10 / 3      // 3 (integer division)
say 10.0 / 3    // 3.3333... (float division)
```

#### Compound Assignment

```
let mut x = 10
x += 5     // x is now 15
x -= 3     // x is now 12
x *= 2     // x is now 24
x /= 4     // x is now 6
```

#### Comparison

| Operator | Description           |
| -------- | --------------------- |
| `==`     | Equal                 |
| `!=`     | Not equal             |
| `<`      | Less than             |
| `>`      | Greater than          |
| `<=`     | Less than or equal    |
| `>=`     | Greater than or equal |

#### Logical

| Operator | Description |
| -------- | ----------- |
| `&&`     | Logical AND |
| `\|\|`   | Logical OR  |
| `!`      | Logical NOT |

### Comments

Single-line comments start with `//`:

```
// This is a comment
let x = 42  // This is also a comment
```

### Newlines as Statement Terminators

Forge uses newlines to separate statements. No semicolons required:

```
let a = 1
let b = 2
let c = a + b
say c
```

This is similar to Go and Python. Opening a `{` allows the statement to span multiple lines.

---

## Chapter 3: Control Flow

### if / else

```
let score = 85

if score >= 90 {
    say "Grade: A"
} else if score >= 80 {
    say "Grade: B"
} else {
    say "Grade: C"
}
```

#### Natural Alternatives

Forge provides `otherwise` and `nah` as aliases for `else`:

```
if ready {
    say "Let's go!"
} otherwise {
    say "Not yet"
}

if done {
    say "Finished"
} nah {
    say "Still working"
}
```

All three — `else`, `otherwise`, `nah` — are semantically identical. Use whichever reads best in your code.

#### if as an Expression

`if` blocks return the value of their last expression, so you can use them on the right side of an assignment:

```
let grade = if score >= 90 { "A" } else { "B" }
```

### when Guards

The `when` statement is Forge's alternative to long `if/else if` chains. It evaluates a subject against a series of conditions:

```
let temp = 72

when temp {
    > 100 -> say "Boiling"
    > 80  -> say "Hot"
    > 60  -> say "Nice"
    > 40  -> say "Cool"
    else  -> say "Cold"
}
```

Each arm is a comparison operator followed by a value, an arrow `->`, and a result expression. The `else` arm is the fallback.

### match (Pattern Matching)

`match` destructures values and binds variables:

```
let result = Ok(42)

match result {
    Ok(value) => say "Got: {value}"
    Err(msg) => say "Error: {msg}"
}
```

Pattern matching works with algebraic data types (Chapter 6), Result types, and Option types:

```
type Color = Red | Green | Blue

let c = Green

match c {
    Red => say "Stop"
    Green => say "Go"
    Blue => say "Sky"
}
```

### for Loops

```
let fruits = ["apple", "banana", "cherry"]

for fruit in fruits {
    say fruit
}
```

#### Natural for-each

```
for each item in [1, 2, 3] {
    say item
}
```

#### Object Iteration

When iterating over an object, you can destructure both key and value:

```
let user = { name: "Alice", age: 30 }

for key, value in user {
    say "{key}: {value}"
}
```

#### Range-based Loops

```
for i in range(0, 5) {
    say i
}
// Prints: 0 1 2 3 4
```

### while Loops

```
let mut count = 0
while count < 5 {
    say count
    count = count + 1
}
```

### repeat Loops

Unique to Forge, `repeat` provides a counted loop with clean syntax:

```
repeat 3 times {
    say "Hello!"
}
```

This is equivalent to `for i in range(0, 3)` but reads more naturally, especially for beginners.

### loop (Infinite)

```
let mut i = 0
loop {
    if i >= 5 { break }
    say i
    i = i + 1
}
```

### break and continue

Both `break` and `continue` work inside `for`, `while`, `repeat`, and `loop`:

```
for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
    if n % 2 == 0 { continue }
    if n > 7 { break }
    say n
}
// Prints: 1, 3, 5, 7
```

---

## Chapter 4: Functions and Closures

### Defining Functions

Forge supports two styles of function definition:

```
// Classic style
fn add(a, b) {
    return a + b
}

// Natural style
define greet(name) {
    return "Hello, {name}!"
}
```

Both are identical in behavior. Use `fn` for conciseness, `define` for readability.

### Parameters and Return Values

Parameters can have type annotations:

```
fn multiply(a: Int, b: Int) -> Int {
    return a * b
}
```

Type annotations are checked by the type checker but not strictly enforced at runtime in the current version. They serve as documentation and help catch bugs early.

### Implicit Returns

The last expression in a function body is implicitly returned:

```
fn square(x) {
    x * x
}

say square(5)  // 25
```

### Closures (Anonymous Functions)

Closures are created with the `fn` keyword without a name:

```
let double = fn(x) { return x * 2 }
say double(21)  // 42
```

Closures capture variables from their enclosing scope:

```
fn make_counter() {
    let mut count = 0
    return fn() {
        count = count + 1
        return count
    }
}

let counter = make_counter()
say counter()  // 1
say counter()  // 2
say counter()  // 3
```

### Higher-Order Functions

Functions are first-class values in Forge. You can pass them as arguments, return them from other functions, and store them in variables:

```
fn apply(f, value) {
    return f(value)
}

fn triple(x) { return x * 3 }

say apply(triple, 14)  // 42
```

### Recursion

Forge fully supports recursive functions:

```
fn factorial(n) {
    if n <= 1 { return 1 }
    return n * factorial(n - 1)
}

say factorial(10)  // 3628800
```

### Decorators

Functions can be annotated with decorators for metadata:

```
@test
fn should_add_numbers() {
    assert_eq(2 + 2, 4)
}

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}
```

Decorators are used by the test runner (`@test`), HTTP server (`@get`, `@post`, `@put`, `@delete`, `@ws`), and server configuration (`@server`).

---

## Chapter 5: Collections

### Arrays

Arrays are ordered, heterogeneous collections:

```
let numbers = [1, 2, 3, 4, 5]
let mixed = [1, "two", true, [4, 5]]
```

#### Accessing Elements

```
let fruits = ["apple", "banana", "cherry"]
say fruits[0]   // apple
say fruits[2]   // cherry
```

#### Built-in Array Operations

```
let nums = [5, 3, 1, 4, 2]

say len(nums)           // 5
push(nums, 6)           // adds 6 to the end
say sort(nums)          // [1, 2, 3, 4, 5, 6]
say reverse(nums)       // [6, 5, 4, 3, 2, 1]
say contains(nums, 3)   // true
```

#### Functional Array Operations

Forge provides `map`, `filter`, and `reduce` as built-in functions:

```
let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

// map: transform every element
let doubled = map(numbers, fn(x) { return x * 2 })
say doubled  // [2, 4, 6, 8, 10, 12, 14, 16, 18, 20]

// filter: keep elements that match a condition
let evens = filter(numbers, fn(x) { return x % 2 == 0 })
say evens  // [2, 4, 6, 8, 10]

// reduce: fold into a single value
let sum = reduce(numbers, 0, fn(acc, x) { return acc + x })
say sum  // 55
```

These can be chained for expressive data pipelines:

```
let result = reduce(
    map(
        filter([1, 2, 3, 4, 5, 6], fn(x) { return x % 2 == 0 }),
        fn(x) { return x * x }
    ),
    0,
    fn(acc, x) { return acc + x }
)
say result  // 56 (4 + 16 + 36)
```

### Objects

Objects are insertion-ordered key-value maps. They use the same syntax as JSON:

```
let user = {
    name: "Alice",
    age: 30,
    role: "engineer",
    active: true
}
```

#### Field Access

```
say user.name    // Alice
say user.age     // 30
```

#### Object Operations

```
say keys(user)     // ["name", "age", "role", "active"]
say values(user)   // ["Alice", 30, "engineer", true]
say len(user)      // 4
```

#### Iteration

```
for key, value in user {
    say "{key} = {value}"
}
```

#### Nested Objects

```
let config = {
    server: {
        host: "0.0.0.0",
        port: 8080
    },
    database: {
        url: "sqlite::memory:",
        pool_size: 5
    }
}

say config.server.port  // 8080
```

### Enumeration

The `enumerate` function adds indices to iteration:

```
let colors = ["red", "green", "blue"]
let indexed = enumerate(colors)
for pair in indexed {
    say pair
}
// [0, "red"]
// [1, "green"]
// [2, "blue"]
```

---

## Chapter 6: Types and Data Modeling

### Algebraic Data Types

Forge supports algebraic data types (ADTs) — types that can be one of several variants:

```
type Color = Red | Green | Blue

type Shape = Circle(Float) | Rect(Float, Float) | Triangle(Float, Float, Float)
```

Unit variants (like `Red`) carry no data. Constructor variants (like `Circle(Float)`) carry associated values.

#### Creating Values

```
let c = Red
let shape = Circle(5.0)
let box = Rect(3.0, 4.0)
```

#### Pattern Matching on ADTs

```
define area(shape) {
    match shape {
        Circle(r) => return 3.14159 * r * r
        Rect(w, h) => return w * h
        Triangle(a, b, c) => {
            let s = (a + b + c) / 2.0
            return math.sqrt(s * (s - a) * (s - b) * (s - c))
        }
    }
}

say area(Circle(5.0))     // 78.53975
say area(Rect(3.0, 4.0))  // 12
```

### Result Types

Forge uses `Result` as its primary error handling mechanism. A `Result` is either `Ok(value)` for success or `Err(message)` for failure:

```
fn parse_age(input) {
    let n = int(input)
    if n < 0 { return Err("age cannot be negative") }
    if n > 150 { return Err("age seems unrealistic") }
    return Ok(n)
}

let result = parse_age("25")
say is_ok(result)       // true
say unwrap(result)      // 25

let bad = parse_age("-5")
say is_err(bad)         // true
say unwrap_or(bad, 0)   // 0
```

#### The ? Operator

The `?` operator propagates errors automatically. If the expression is `Err`, the function returns that error immediately:

```
fn process_user_input(input) {
    let age = parse_age(input)?
    return Ok("User is {age} years old")
}

match process_user_input("-5") {
    Ok(msg) => say msg
    Err(e) => say "Error: {e}"
}
// Error: age cannot be negative
```

This eliminates nested `if is_err(...)` checks and keeps error handling clean.

### Option Types

For values that may or may not exist, use `Some` and `None`:

```
let x = Some(42)
let y = None

say is_some(x)  // true
say is_none(y)  // true

match x {
    Some(val) => say "Got: {val}"
    None => say "Nothing"
}
```

### The must Keyword

When you are certain a Result will succeed and want to crash if it doesn't, use `must`:

```
let value = must parse_age("25")
say value  // 25

// This will crash with an error:
// let bad = must parse_age("-5")
```

`must` is intentionally aggressive — it signals to both the runtime and the reader that failure is not expected here.

### Interfaces

Forge supports Go-style implicit interfaces:

```
interface Printable {
    fn to_string() -> String
}
```

A type satisfies an interface if it has all the required methods. No explicit `implements` declaration is needed.

### Type Checking

The `type()` and `typeof()` functions return the type of a value as a string:

```
say type(42)          // Int
say type("hello")     // String
say type([1, 2, 3])   // Array
say type(true)        // Bool
say typeof(3.14)      // Float
```

---

## Chapter 7: Error Handling

### Philosophy

Forge treats errors as values, not exceptions. There is no `throw`, no stack unwinding, no invisible control flow. When a function can fail, it returns a `Result`. When you call that function, you handle the result explicitly.

This makes error paths visible in the code. You can see exactly where errors are handled and where they propagate.

### Result Construction

```
fn divide(a, b) {
    if b == 0 {
        return Err("division by zero")
    }
    return Ok(a / b)
}
```

### Result Inspection

```
let r = divide(10, 3)

say is_ok(r)           // true
say is_err(r)          // false
say unwrap(r)          // 3
say unwrap_or(r, -1)   // 3

let bad = divide(10, 0)
say unwrap_or(bad, -1) // -1
```

### Error Propagation with ?

The `?` operator is the idiomatic way to propagate errors up the call stack:

```
fn read_config(path) {
    let content = fs.read(path)?
    let data = json.parse(content)?
    return Ok(data)
}
```

If `fs.read` returns an `Err`, `read_config` immediately returns that same `Err`. If it returns `Ok`, the inner value is unwound and bound to `content`.

### try/catch Blocks

For cases where you want to catch errors from a block of code:

```
try {
    let data = fs.read("config.json")
    let config = json.parse(data)
    say config
} catch err {
    say "Failed to load config: {err}"
}
```

### safe Blocks

`safe` blocks suppress all errors and return `null` on failure:

```
safe {
    let data = fs.read("maybe-missing.txt")
    say data
}
// If the file doesn't exist, nothing happens (no crash)
```

### check Validation

The `check` statement validates conditions declaratively:

```
let email = "user@example.com"
check email is not empty
check email contains "@"
```

If the check fails, it raises a runtime error with a descriptive message.

### Error Messages

Forge provides helpful error messages with source context:

```
$ forge -e 'say naem'

error: undefined variable: 'naem'
  hint: did you mean 'name'?
```

The language uses Levenshtein distance to suggest corrections for misspelled variables, and includes source location information for all errors.

---

## Chapter 8: Output — The Fun Trio

One of Forge's most distinctive features is its three-tier output system:

### say

Standard output. Prints the value as-is:

```
say "Hello, World!"     // Hello, World!
say 42                  // 42
say [1, 2, 3]          // [1, 2, 3]
```

### yell

Converts to uppercase and adds emphasis:

```
yell "hello world"     // HELLO WORLD!
yell "forge is great"  // FORGE IS GREAT!
```

### whisper

Converts to lowercase and adds a trailing ellipsis:

```
whisper "HELLO WORLD"   // hello world...
whisper "FORGE"         // forge...
```

These three output functions share the same signature as `println` — they accept any value and print it with a newline. The difference is purely in formatting.

### Classic Output

The traditional `print` and `println` functions are also available:

```
print("no newline")
println("with newline")
```

---

# Part II: The Standard Library

Forge ships with 15 built-in modules. No package manager, no `npm install`, no `pip install` — they're part of the language.

---

## Chapter 9: math

The `math` module provides mathematical constants and functions.

### Constants

| Name       | Value               | Description       |
| ---------- | ------------------- | ----------------- |
| `math.pi`  | `3.141592653589793` | Pi                |
| `math.e`   | `2.718281828459045` | Euler's number    |
| `math.inf` | `Infinity`          | Positive infinity |

### Functions

| Function              | Description       | Example                       |
| --------------------- | ----------------- | ----------------------------- |
| `math.sqrt(n)`        | Square root       | `math.sqrt(144)` → `12`       |
| `math.pow(base, exp)` | Exponentiation    | `math.pow(2, 10)` → `1024`    |
| `math.abs(n)`         | Absolute value    | `math.abs(-42)` → `42`        |
| `math.max(a, b)`      | Maximum           | `math.max(3, 7)` → `7`        |
| `math.min(a, b)`      | Minimum           | `math.min(3, 7)` → `3`        |
| `math.floor(n)`       | Round down        | `math.floor(3.7)` → `3`       |
| `math.ceil(n)`        | Round up          | `math.ceil(3.2)` → `4`        |
| `math.round(n)`       | Round nearest     | `math.round(3.5)` → `4`       |
| `math.random()`       | Random 0..1       | `math.random()` → `0.547...`  |
| `math.sin(n)`         | Sine (radians)    | `math.sin(math.pi / 2)` → `1` |
| `math.cos(n)`         | Cosine (radians)  | `math.cos(0)` → `1`           |
| `math.tan(n)`         | Tangent (radians) | `math.tan(math.pi / 4)` → `1` |
| `math.log(n)`         | Natural logarithm | `math.log(math.e)` → `1`      |

All math functions accept both `Int` and `Float` arguments and return `Float` where appropriate.

---

## Chapter 10: fs (File System)

The `fs` module provides synchronous file system operations.

### Reading and Writing

```
// Write a file
fs.write("notes.txt", "Hello from Forge!")

// Read a file
let content = fs.read("notes.txt")
say content  // Hello from Forge!

// Append to a file
fs.append("notes.txt", "\nSecond line")
```

### File Information

```
say fs.exists("notes.txt")    // true
say fs.size("notes.txt")      // 33 (bytes)
say fs.ext("photo.jpg")       // jpg
```

### Directory Operations

```
fs.mkdir("output")
let files = fs.list(".")
say files  // [...list of files...]
```

### File Management

```
fs.copy("notes.txt", "backup.txt")
fs.rename("backup.txt", "archive.txt")
fs.remove("archive.txt")
```

### JSON Files

```
let config = { port: 8080, debug: true }
fs.write_json("config.json", config)

let loaded = fs.read_json("config.json")
say loaded.port  // 8080
```

---

## Chapter 11: crypto

The `crypto` module provides cryptographic hashing and encoding functions.

### Hashing

```
say crypto.sha256("hello")
// 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824

say crypto.md5("hello")
// 5d41402abc4b2a76b9719d911017c592
```

### Base64 Encoding

```
let encoded = crypto.base64_encode("secret message")
say encoded  // c2VjcmV0IG1lc3NhZ2U=

let decoded = crypto.base64_decode(encoded)
say decoded  // secret message
```

### Hex Encoding

```
let hex = crypto.hex_encode("data")
say hex  // 64617461

let raw = crypto.hex_decode(hex)
say raw  // data
```

---

## Chapter 12: db (SQLite) and pg (PostgreSQL)

### SQLite

Forge has SQLite built in — no external database server needed:

```
// Open a database (use ":memory:" for in-memory)
db.open("app.db")

// Create tables
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")

// Insert data
db.execute("INSERT INTO users (name, email) VALUES (\"Alice\", \"alice@example.com\")")

// Query data
let users = db.query("SELECT * FROM users")
say users
// [{ id: 1, name: "Alice", email: "alice@example.com" }]

// Close the connection
db.close()
```

### PostgreSQL

For production databases, use the `pg` module:

```
pg.connect("host=localhost dbname=myapp user=postgres password=secret")

let users = pg.query("SELECT * FROM users WHERE active = true")
for each user in users {
    say "{user.name}: {user.email}"
}

pg.execute("UPDATE users SET last_login = NOW() WHERE id = 1")

pg.close()
```

---

## Chapter 13: json

The `json` module handles serialization and deserialization:

```
// Parse JSON string into a Forge object
let data = json.parse("{\"name\": \"Forge\", \"version\": 2}")
say data.name     // Forge
say data.version  // 2

// Convert Forge object to JSON string
let text = json.stringify(data)
say text  // {"name":"Forge","version":2}

// Pretty-print with indentation
say json.pretty(data)
// {
//   "name": "Forge",
//   "version": 2
// }
```

---

## Chapter 14: regex

The `regex` module provides regular expression operations:

```
// Test if a pattern matches
say regex.test("[0-9]+", "abc123")  // true

// Find the first match
say regex.find("[0-9]+", "abc123def456")  // 123

// Find all matches
say regex.find_all("[0-9]+", "a1b2c3")  // ["1", "2", "3"]

// Replace matches
say regex.replace("[aeiou]", "hello world", "*")  // h*ll* w*rld

// Split by pattern
say regex.split("[,;]", "a,b;c,d")  // ["a", "b", "c", "d"]
```

---

## Chapter 15: env

The `env` module reads and writes environment variables:

```
say env.get("HOME")       // /Users/username
say env.get("SHELL")      // /bin/bash

env.set("MY_VAR", "hello")
say env.get("MY_VAR")     // hello

say env.has("PATH")       // true

let all_keys = env.keys()
say len(all_keys)          // number of env vars
```

---

## Chapter 16: csv

The `csv` module handles comma-separated value data:

```
// Parse a CSV string into an array of objects
let data = csv.parse("name,age\nAlice,30\nBob,25")
say data
// [{ name: "Alice", age: "30" }, { name: "Bob", age: "25" }]

// Convert an array of objects to CSV
let text = csv.stringify([
    { name: "Alice", age: 30 },
    { name: "Bob", age: 25 }
])
say text
// name,age
// Alice,30
// Bob,25

// Read/write CSV files
csv.write("people.csv", data)
let loaded = csv.read("people.csv")
```

---

## Chapter 17: log

The `log` module provides structured logging with severity levels:

```
log.info("Server started on port 8080")
log.warn("Connection pool running low")
log.error("Failed to connect to database")
log.debug("Request payload: {data}")
```

Output is color-coded:

- `info` — blue
- `warn` — yellow
- `error` — red
- `debug` — gray

---

## Chapter 18: term (Terminal UI)

The `term` module is one of Forge's most distinctive features — a complete terminal UI toolkit built into the language.

### Colors

```
say term.red("Error: something went wrong")
say term.green("Success!")
say term.blue("Info: processing...")
say term.yellow("Warning: disk space low")
say term.cyan("Hint: try --verbose")
say term.magenta("Debug: value = 42")
say term.bold("Important message")
say term.dim("Less important")
```

### Tables

Render arrays of objects as formatted tables:

```
term.table([
    { name: "Alice", role: "Engineer", level: 5 },
    { name: "Bob", role: "Designer", level: 3 },
    { name: "Carol", role: "Manager", level: 7 }
])
```

Output:

```
 name  | role     | level
-------+----------+-------
 Alice | Engineer | 5
 Bob   | Designer | 3
 Carol | Manager  | 7
```

### Data Visualization

```
// Sparkline (inline chart)
term.sparkline([1, 5, 3, 8, 2, 9, 4, 7])

// Progress bar
term.bar("Upload", 75, 100)
// Upload [████████████████████████░░░░░░] 75%

// Banner text
term.banner("FORGE")
```

### Rich Output

```
term.box("Important message in a box")
term.gradient("Beautiful gradient text")
term.hr()  // horizontal rule

// Status messages
term.success("All tests passed!")
term.warning("Disk space low")
term.error("Connection refused")
term.info("Processing 42 records")
```

### Interactive

```
let answer = term.confirm("Continue?")
let choice = term.menu(["Option A", "Option B", "Option C"])
```

### Effects

```
term.countdown(5)       // 5... 4... 3... 2... 1...
term.typewriter("Typing effect...")
term.beep()              // system bell
term.clear()             // clear screen
```

### Emoji

```
say term.emoji("rocket")     // 🚀
say term.emoji("check")      // ✅
say term.emoji("fire")       // 🔥
term.emojis()                 // list all available emoji names
```

---

## Chapter 19: io and exec

### io

```
let name = io.prompt("What is your name? ")
say "Hello, {name}!"
```

### exec (Command Execution)

Forge provides three ways to run shell commands:

#### run_command

Runs a command with explicit arguments:

```
let result = run_command("ls", ["-la"])
say result
```

#### shell

Runs a command through `/bin/sh` and returns a full result object:

```
let r = shell("cat /etc/hosts | grep localhost | wc -l")
say r.stdout   // the output
say r.stderr   // any error output
say r.status   // exit code (0 = success)
say r.ok       // true if exit code is 0
```

#### sh

Shorthand that returns just the stdout as a string:

```
say sh("whoami")               // username
say sh("date +%Y-%m-%d")      // 2026-02-28
say sh("ls -la | head -5")    // first 5 lines of ls output
```

`shell()` and `sh()` support pipes, redirects, variable expansion — everything your shell supports.

---

# Part III: Building Real Things

---

## Chapter 20: HTTP Servers

### The Decorator Model

Forge uses decorators to define HTTP routes. There's no router object, no middleware chain, no request handler class — just functions with annotations:

```
@server(port: 3000)

@get("/")
fn home() -> Json {
    return { message: "Welcome to my API" }
}
```

The `@server` decorator configures the HTTP server. Route decorators (`@get`, `@post`, `@put`, `@delete`) bind functions to URL patterns.

### Route Parameters

Use `:param` syntax for dynamic URL segments:

```
@get("/users/:id")
fn get_user(id: String) -> Json {
    return { id: id, name: "User {id}" }
}

// GET /users/42 → { "id": "42", "name": "User 42" }
```

Multiple parameters:

```
@get("/repos/:owner/:repo")
fn get_repo(owner: String, repo: String) -> Json {
    return { owner: owner, repo: repo }
}
```

### Request Bodies

POST and PUT handlers receive the request body as a JSON parameter:

```
@post("/users")
fn create_user(body: Json) -> Json {
    say "Creating user: {body.name}"
    return { created: true, name: body.name }
}
```

### Query Parameters

Query string parameters are automatically available:

```
@get("/search")
fn search(q: String) -> Json {
    return { query: q, results: [] }
}

// GET /search?q=forge → { "query": "forge", "results": [] }
```

### WebSocket Support

```
@server(port: 8080)

@ws("/chat")
fn on_message(msg: String) -> String {
    return "Echo: {msg}"
}
```

### A Complete API Example

```
db.open("app.db")
db.execute("CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, text TEXT)")

@server(port: 3000)

@get("/notes")
fn list_notes() -> Json {
    return db.query("SELECT * FROM notes")
}

@post("/notes")
fn create_note(body: Json) -> Json {
    db.execute("INSERT INTO notes (text) VALUES (\"" + body.text + "\")")
    return { saved: true }
}

@delete("/notes/:id")
fn delete_note(id: String) -> Json {
    db.execute("DELETE FROM notes WHERE id = " + id)
    return { deleted: true }
}
```

This is a complete CRUD API with persistent storage — in 18 lines of Forge.

---

## Chapter 21: HTTP Client

### fetch

The simplest way to make HTTP requests:

```
let resp = fetch("https://api.github.com/repos/rust-lang/rust")
say resp.status  // 200
say resp.ok      // true
say resp.body    // response body as string
```

### Module Functions

For more control, use the `http` module:

```
// GET
let resp = http.get("https://httpbin.org/get")

// POST with JSON body
let resp = http.post("https://httpbin.org/post", { name: "Forge" })

// PUT
let resp = http.put("https://httpbin.org/put", { updated: true })

// DELETE
let resp = http.delete("https://httpbin.org/delete")

// PATCH
let resp = http.patch("https://httpbin.org/patch", { field: "value" })

// HEAD (headers only)
let resp = http.head("https://httpbin.org/get")
```

### Downloading Files

```
download "https://example.com/file.zip" to "/tmp/file.zip"
```

Or via the module:

```
http.download("https://example.com/file.zip", "/tmp/file.zip")
```

### Web Crawling

```
crawl "https://example.com"
```

Or via the module:

```
let content = http.crawl("https://example.com")
say content  // page text content
```

---

## Chapter 22: Data Processing

Forge excels at data transformation tasks that typically require pandas or dedicated ETL tools.

### CSV to Database Pipeline

```
// Read CSV data
let sales = csv.read("sales.csv")

// Load into SQLite
db.open(":memory:")
db.execute("CREATE TABLE sales (product TEXT, amount REAL, region TEXT)")

for each row in sales {
    db.execute("INSERT INTO sales VALUES (\"" + row.product + "\", " + row.amount + ", \"" + row.region + "\")")
}

// Query and analyze
let by_region = db.query("SELECT region, SUM(amount) as total FROM sales GROUP BY region ORDER BY total DESC")

// Display results
term.table(by_region)

// Visualize
let amounts = map(by_region, fn(r) { return float(r.total) })
term.sparkline(amounts)

// Export
csv.write("summary.csv", by_region)
say json.pretty(by_region)

db.close()
```

### JSON API to Report

```
let resp = fetch("https://api.example.com/metrics")
let data = json.parse(resp.body)

let filtered = filter(data.items, fn(item) { return item.value > 100 })
let sorted = sort(filtered)

term.table(sorted)
term.bar("Active", len(filtered), len(data.items))

fs.write_json("report.json", filtered)
```

---

## Chapter 23: DevOps and Scripting

### System Health Checks

```
say term.bold("=== System Health Check ===")

let user = sh("whoami")
let host = sh("hostname")
let os = sh("uname -s")
let arch = sh("uname -m")

say "  User:     {user}"
say "  Hostname: {host}"
say "  OS:       {os}"
say "  Arch:     {arch}"

let disk = shell("df -h / | tail -1")
say "  Disk:     {disk.stdout}"

term.success("Health check passed!")
```

### Configuration Management

```
let config = fs.read_json("deploy.json")

if config.environment == "production" {
    log.warn("Deploying to PRODUCTION")
    let answer = term.confirm("Are you sure?")
    if !answer { say "Aborted" }
}

let hash = crypto.sha256(json.stringify(config))
say "Config hash: {hash}"
```

### Process Management

```
let result = shell("pgrep -f myservice")
if result.ok {
    say "Service is running (PID: {result.stdout})"
} otherwise {
    say "Service is down, starting..."
    shell("systemctl start myservice")
}
```

---

## Chapter 24: AI Integration

### ask

The `ask` function sends a prompt to an LLM:

```
let response = ask("Explain recursion in one sentence")
say response
```

Requires the `FORGE_AI_KEY` or `OPENAI_API_KEY` environment variable.

### prompt Templates

Define reusable prompt templates:

```
prompt summarize(text) {
    system: "You are a concise summarizer."
    user: "Summarize this: {text}"
    returns: "A brief summary"
}
```

### agent Blocks

Define autonomous agents with tools and goals:

```
agent researcher(topic) {
    tools: ["search", "read"]
    goal: "Research {topic} thoroughly"
    max_steps: 5
}
```

### Interactive Chat

```
$ forge chat

  ╔══════════════════════════════════════╗
  ║        Forge AI Chat                 ║
  ╚══════════════════════════════════════╝

  Connected! Type your message, or 'exit' to quit.
  Type '/forge <code>' to run Forge code inline.

> What's the capital of France?
  Paris is the capital of France.

> /forge say math.sqrt(144)
  12
```

---

# Part IV: Under the Hood

---

## Chapter 25: Architecture

### The Compilation Pipeline

Every Forge program goes through these stages:

```
Source Code (.fg)
    ↓
  Lexer (tokenization)
    ↓
  Tokens
    ↓
  Parser (syntax analysis)
    ↓
  Abstract Syntax Tree (AST)
    ↓
  Type Checker (optional warnings)
    ↓
  Interpreter (tree-walk execution)
    or
  Bytecode Compiler → VM (with --vm flag)
```

### The Lexer

The lexer (`src/lexer/`) converts source text into a stream of tokens. It handles:

- Keywords (80+ including natural-language alternatives)
- String interpolation (parsing `{expr}` inside strings)
- Operators and delimiters
- Newline significance (newlines are tokens, not whitespace)
- Source positions for error reporting

### The Parser

The parser (`src/parser/`) builds an Abstract Syntax Tree using recursive descent with Pratt precedence for expressions. Key design choices:

- **Newline-aware**: newlines terminate statements, but are skipped after opening delimiters
- **Keyword flexibility**: keywords like `set`, `to`, `each` can also appear as field names
- **Decorator handling**: `@name(args)` decorators attach to the next function definition
- **Dual syntax**: classic and natural keywords parse to the same AST nodes

### The AST

The AST (`src/parser/ast.rs`) defines two core enums:

- `Stmt` — 24 statement variants (Let, Assign, FnDef, If, For, While, Match, When, TryCatch, etc.)
- `Expr` — 22 expression variants (Int, String, BinOp, Call, FieldAccess, Lambda, Try, Must, etc.)

### The Interpreter

The tree-walk interpreter (`src/interpreter/`) walks the AST and executes it directly. It manages:

- **Environment**: a scope stack (Vec of HashMaps) with push/pop for blocks
- **Closures**: functions capture the environment at definition time
- **Builtins**: 230+ native functions registered at startup
- **Error handling**: Result types, `?` propagation, and descriptive error messages

### The Bytecode VM

The bytecode VM (`src/vm/`) provides a faster execution path:

- **Register-based**: unlike stack-based VMs, operands stay in registers
- **Bytecode compiler**: translates AST → instruction sequences
- **Mark-sweep GC**: manages heap-allocated values
- **Green thread scheduler**: scaffolded for future concurrent execution

Use `--vm` to opt into VM execution:

```
forge run --vm myprogram.fg
```

---

## Chapter 26: Tooling

### forge fmt (Formatter)

Automatically formats Forge source files:

```
$ forge fmt                    # format all .fg files in current directory
$ forge fmt src/main.fg        # format specific files
```

The formatter normalizes indentation (4 spaces per level), removes trailing whitespace, and ensures consistent brace placement.

### forge test (Test Runner)

Write tests with the `@test` decorator:

```
@test
define should_add() {
    assert(1 + 1 == 2)
    assert_eq(2 * 3, 6)
}

@test
define should_handle_strings() {
    let name = "Forge"
    assert(len(name) == 5)
    assert(starts_with(name, "For"))
}
```

Run tests:

```
$ forge test

  tests/basic_test.fg
    ok    should_add (0ms)
    ok    should_handle_strings (0ms)

  25 passed, 0 failed, 25 total
```

### forge new (Project Scaffolding)

```
$ forge new myproject

  Created new Forge project 'myproject'

  myproject/
    forge.toml
    main.fg
    tests/
      basic_test.fg
    .gitignore

  Get started:
    cd myproject
    forge run main.fg
    forge test
```

### forge build (Bytecode Compilation)

Compiles source to bytecode (for inspection or faster loading):

```
$ forge build myprogram.fg

Compiled myprogram.fg -> myprogram.fgc
  42 instructions
  8 constants
  3 prototypes
  16 max registers
```

### forge install (Package Manager)

Install packages from git repositories or local paths:

```
$ forge install https://github.com/user/forge-utils.git
  Installing forge-utils from https://github.com/user/forge-utils.git
  Installed forge-utils

$ forge install ./local-package
  Installed local-package from ./local-package
```

Packages are stored in `.forge/packages/`.

### forge lsp (Language Server)

Start a Language Server Protocol server for editor integration:

```
$ forge lsp
```

The LSP provides diagnostics and basic editor features. A VS Code extension is available in `editors/vscode/`.

---

## Chapter 27: The Forge Project File

Every Forge project has a `forge.toml` manifest:

```toml
[project]
name = "myapp"
version = "1.0.0"
description = "My Forge application"

[test]
directory = "tests"
```

The manifest configures:

- Project metadata (name, version, description)
- Test directory location
- Future: dependencies, build settings, scripts

---

# Appendices

---

## Appendix A: Complete Keyword Reference

### Classic Keywords

| Keyword           | Purpose              | Example                    |
| ----------------- | -------------------- | -------------------------- |
| `let`             | Variable declaration | `let x = 42`               |
| `mut`             | Mutable modifier     | `let mut x = 0`            |
| `fn`              | Function definition  | `fn add(a, b) { }`         |
| `return`          | Return value         | `return x + 1`             |
| `if`              | Conditional          | `if x > 0 { }`             |
| `else`            | Else branch          | `} else { }`               |
| `match`           | Pattern matching     | `match x { ... }`          |
| `for`             | For loop             | `for i in items { }`       |
| `in`              | Iterator keyword     | `for i in items`           |
| `while`           | While loop           | `while x > 0 { }`          |
| `loop`            | Infinite loop        | `loop { break }`           |
| `break`           | Exit loop            | `break`                    |
| `continue`        | Skip iteration       | `continue`                 |
| `type`            | Type definition      | `type Color = Red \| Blue` |
| `struct`          | Struct definition    | `struct Point { x: Int }`  |
| `interface`       | Interface definition | `interface Printable { }`  |
| `import`          | Import module        | `import "utils"`           |
| `spawn`           | Background thread    | `spawn { work() }`         |
| `true` / `false`  | Boolean literals     | `let x = true`             |
| `try` / `catch`   | Error handling       | `try { } catch e { }`      |
| `async` / `await` | Async functions      | `async fn load() { }`      |

### Natural Keywords

| Keyword     | Classic Equivalent   | Example                  |
| ----------- | -------------------- | ------------------------ |
| `set`       | `let`                | `set name to "Forge"`    |
| `to`        | `=` (in set context) | `set x to 42`            |
| `change`    | reassignment         | `change x to x + 1`      |
| `define`    | `fn`                 | `define greet(name) { }` |
| `otherwise` | `else`               | `} otherwise { }`        |
| `nah`       | `else`               | `} nah { }`              |
| `each`      | (in for)             | `for each item in list`  |
| `repeat`    | counted loop         | `repeat 3 times { }`     |
| `times`     | (with repeat)        | `repeat N times { }`     |
| `say`       | `println`            | `say "hello"`            |
| `yell`      | uppercase println    | `yell "loud"`            |
| `whisper`   | lowercase println    | `whisper "quiet"`        |
| `grab`      | fetch                | `grab data from "url"`   |
| `wait`      | sleep                | `wait 2 seconds`         |
| `forge`     | async function       | `forge fn load() { }`    |
| `hold`      | await                | `hold fetch("url")`      |
| `emit`      | yield                | `emit value`             |
| `unpack`    | destructure          | `unpack {a, b} from obj` |

### Innovation Keywords

| Keyword    | Purpose               | Example                         |
| ---------- | --------------------- | ------------------------------- |
| `when`     | Multi-way conditional | `when x { > 10 -> "big" }`      |
| `must`     | Assert success        | `must parse("42")`              |
| `check`    | Validation            | `check email is not empty`      |
| `safe`     | Error suppression     | `safe { risky() }`              |
| `timeout`  | Time-limited block    | `timeout 5 seconds { }`         |
| `retry`    | Retry on failure      | `retry 3 times { }`             |
| `schedule` | Periodic execution    | `schedule every 60 seconds { }` |
| `watch`    | File watching         | `watch "config.json" { }`       |
| `ask`      | LLM query             | `ask "explain recursion"`       |
| `prompt`   | Prompt template       | `prompt summarize(text) { }`    |
| `download` | File download         | `download "url" to "path"`      |
| `crawl`    | Web scraping          | `crawl "https://example.com"`   |
| `freeze`   | Immutable copy        | `freeze data`                   |

---

## Appendix B: Built-in Functions Quick Reference

### Output

| Function         | Description                |
| ---------------- | -------------------------- |
| `print(value)`   | Print without newline      |
| `println(value)` | Print with newline         |
| `say(value)`     | Print with newline (alias) |
| `yell(value)`    | Print uppercase with !     |
| `whisper(value)` | Print lowercase with ...   |

### Type Conversion

| Function        | Description             |
| --------------- | ----------------------- |
| `str(value)`    | Convert to String       |
| `int(value)`    | Convert to Int          |
| `float(value)`  | Convert to Float        |
| `type(value)`   | Get type name as String |
| `typeof(value)` | Get type name (alias)   |

### Collections

| Function                      | Description                        |
| ----------------------------- | ---------------------------------- |
| `len(collection)`             | Length of array, string, or object |
| `push(array, value)`          | Append to array                    |
| `pop(array)`                  | Remove and return last element     |
| `keys(object)`                | Get object keys as array           |
| `values(object)`              | Get object values as array         |
| `contains(collection, value)` | Check membership                   |
| `range(start, end)`           | Generate integer range             |
| `enumerate(array)`            | Add indices to array               |

### Functional

| Function                  | Description            |
| ------------------------- | ---------------------- |
| `map(array, fn)`          | Transform each element |
| `filter(array, fn)`       | Keep matching elements |
| `reduce(array, init, fn)` | Fold into single value |
| `sort(array)`             | Sort ascending         |
| `reverse(array)`          | Reverse order          |

### Strings

| Function                      | Description            |
| ----------------------------- | ---------------------- |
| `split(string, delimiter)`    | Split into array       |
| `join(array, delimiter)`      | Join array into string |
| `replace(string, old, new)`   | Replace occurrences    |
| `starts_with(string, prefix)` | Check prefix           |
| `ends_with(string, suffix)`   | Check suffix           |

### Results and Options

| Function                     | Description                    |
| ---------------------------- | ------------------------------ |
| `Ok(value)`                  | Create success Result          |
| `Err(message)`               | Create error Result            |
| `is_ok(result)`              | Check if Ok                    |
| `is_err(result)`             | Check if Err                   |
| `unwrap(result)`             | Extract value (crashes on Err) |
| `unwrap_or(result, default)` | Extract or use default         |
| `Some(value)`                | Create Option with value       |
| `None`                       | Create empty Option            |
| `is_some(option)`            | Check if has value             |
| `is_none(option)`            | Check if empty                 |

### System

| Function                      | Description                      |
| ----------------------------- | -------------------------------- |
| `time()`                      | Current timestamp                |
| `uuid()`                      | Generate UUID v4                 |
| `exit(code)`                  | Exit with code                   |
| `input(prompt)`               | Read line from stdin             |
| `run_command(cmd, args)`      | Execute command                  |
| `shell(command)`              | Run via /bin/sh (returns object) |
| `sh(command)`                 | Run via /bin/sh (returns stdout) |
| `fetch(url)`                  | HTTP GET request                 |
| `assert(condition)`           | Assert truthy                    |
| `assert_eq(a, b)`             | Assert equality                  |
| `wait(seconds)`               | Sleep                            |
| `satisfies(value, interface)` | Check interface                  |

---

## Appendix C: Operator Precedence

From lowest to highest precedence:

| Level | Operators            | Associativity |
| ----- | -------------------- | ------------- |
| 1     | `\|\|`               | Left          |
| 2     | `&&`                 | Left          |
| 3     | `==`, `!=`           | Left          |
| 4     | `<`, `>`, `<=`, `>=` | Left          |
| 5     | `+`, `-`             | Left          |
| 6     | `*`, `/`, `%`        | Left          |
| 7     | `!`, `-` (unary)     | Prefix        |
| 8     | `.`, `[i]`, `()`     | Left          |
| 9     | `?`                  | Postfix       |

---

## Appendix D: CLI Reference

```
USAGE:
    forge [OPTIONS] [COMMAND]

COMMANDS:
    run <file>        Run a Forge source file
    repl              Start the interactive REPL
    version           Show version information
    fmt [files...]    Format Forge source files
    test [dir]        Run tests (default: tests/)
    new <name>        Create a new Forge project
    build <file>      Compile to bytecode
    install <source>  Install a package
    lsp               Start Language Server Protocol server
    learn [lesson]    Interactive tutorials (30 lessons)
    chat              AI-powered chat assistant

OPTIONS:
    -e <code>         Evaluate inline code
    --vm              Use bytecode VM (experimental)
    -h, --help        Print help
    -V, --version     Print version

EXAMPLES:
    forge                              Start REPL
    forge run hello.fg                 Run a program
    forge -e 'say "Hello!"'           Evaluate inline
    forge test                         Run all tests
    forge new myproject                Create project
    forge learn 1                      Start lesson 1
```

---

## Appendix E: Error Messages

Forge provides context-rich error messages. Here are common errors and what they mean:

### undefined variable

```
error: undefined variable: 'naem'
  hint: did you mean 'name'?
```

You used a variable that doesn't exist. Forge suggests similar names using edit distance.

### unexpected token

```
error: unexpected token: Semicolon
```

Forge uses newlines as statement separators, not semicolons. Remove the `;` and use a newline.

### division by zero

```
error: division by zero
  hint: check that the divisor is not zero before dividing
```

### type mismatch

```
warning: type mismatch: expected Int, got String
```

A type annotation doesn't match the actual value. This is a warning, not an error — the code will still run.

---

## Appendix F: Project Statistics

| Metric                   | Value          |
| ------------------------ | -------------- |
| Implementation language  | Rust           |
| Lines of Rust source     | ~26,000        |
| Source files             | 56             |
| Standard library modules | 16             |
| Built-in functions       | 230+           |
| Language keywords        | 80+            |
| Rust unit tests          | 488            |
| Forge integration tests  | 334            |
| Interactive lessons      | 30             |
| Example programs         | 12             |
| Unsafe code blocks       | 0              |
| External dependencies    | 22 Rust crates |

---

## Appendix G: Comparison with Other Languages

### Forge vs. Python

| Feature         | Python                 | Forge                        |
| --------------- | ---------------------- | ---------------------------- |
| HTTP server     | Flask/Django (install) | Built-in (`@server`, `@get`) |
| HTTP client     | `requests` (install)   | Built-in (`fetch()`)         |
| Database        | `sqlite3` (import)     | Built-in (`db.open()`)       |
| JSON            | `import json`          | Built-in (`json.parse()`)    |
| Crypto          | `import hashlib`       | Built-in (`crypto.sha256()`) |
| CSV             | `import csv`           | Built-in (`csv.parse()`)     |
| Regex           | `import re`            | Built-in (`regex.test()`)    |
| Error handling  | try/except             | Result types with `?`        |
| Type system     | Optional (mypy)        | Gradual (built-in checker)   |
| Concurrency     | GIL-limited            | Green threads (planned)      |
| Package manager | pip + venv             | `forge install`              |
| Formatter       | `black` (install)      | `forge fmt` (built-in)       |
| Test runner     | `pytest` (install)     | `forge test` (built-in)      |

### Forge vs. JavaScript/Node.js

| Feature            | Node.js                    | Forge                           |
| ------------------ | -------------------------- | ------------------------------- |
| HTTP server        | Express (install)          | Built-in (`@server`)            |
| Type safety        | TypeScript (separate)      | Built-in type checker           |
| `null` gotchas     | `null`, `undefined`, `NaN` | No null by default, Option type |
| Package management | npm (1000s of deps)        | 16 built-in modules             |
| Error handling     | try/catch + callbacks      | Result types with `?`           |
| Build step         | Webpack/Vite/etc.          | None needed                     |

### Forge vs. Go

| Feature           | Go                           | Forge                        |
| ----------------- | ---------------------------- | ---------------------------- |
| Syntax complexity | Moderate                     | Simple (reads like English)  |
| HTTP server       | `net/http` (verbose)         | `@server` + `@get` (3 lines) |
| Error handling    | `if err != nil` (repetitive) | `?` operator (one character) |
| Generics          | Recently added               | Parsed (enforcement planned) |
| Learning curve    | Moderate                     | Low (30 built-in lessons)    |
| Shell integration | `os/exec` (verbose)          | `sh("command")` (one line)   |

---

_End of Book_

---

**About the Author**

Archith Rapaka is a software engineer and programming language designer. Forge is his vision for a programming language that puts the internet first — where HTTP, databases, and cryptography are language primitives, not library imports. He believes that programming languages should be as natural to read as they are powerful to write.

---

_Programming Forge: The Internet-Native Language That Reads Like English_
_First Edition, 2026_
_MIT License_
