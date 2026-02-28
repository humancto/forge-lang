# Part I: Foundations

---

_Programming Forge_ by Archith Rapaka

---

## Chapter 1: Getting Started

Forge is a programming language built for the internet age. This chapter introduces the language, walks you through installation, and gets your first program running in under five minutes. By the end, you will have written and executed Forge code, explored the interactive REPL, and discovered the built-in tutorial system that ships with every installation.

### What Is Forge?

Forge is a general-purpose programming language written in Rust. It was designed with a single guiding principle: the things you do most often on the internet — making HTTP requests, querying databases, hashing passwords, parsing JSON — should be built into the language itself, not buried in third-party packages.

Most languages treat the network as an afterthought. You install a language, then install a web framework, then install an HTTP client, then install a JSON library, then install a database driver. Forge ships all of that out of the box. A REST API server is four lines. A database query is two.

Forge also reads like English. Every construct has two spellings: a classic syntax familiar to anyone who has written JavaScript or Python, and a natural-language syntax that reads like prose. Both compile to the same thing. You pick whichever feels right, and you can mix them freely in the same file.

Here is how Forge compares to languages you may already know:

| Feature               | Python    | JavaScript        | Go              | Rust           | Forge                          |
| --------------------- | --------- | ----------------- | --------------- | -------------- | ------------------------------ |
| HTTP client built in  | No        | `fetch` (browser) | `net/http`      | No             | Yes (`fetch`, `http.get`)      |
| HTTP server built in  | No        | No                | `net/http`      | No             | Yes (`@server`, `@get`)        |
| Database built in     | No        | No                | `database/sql`  | No             | Yes (`db.open`, `pg.connect`)  |
| Crypto built in       | `hashlib` | `crypto` (Node)   | `crypto`        | No             | Yes (`crypto.sha256`)          |
| Terminal UI built in  | No        | No                | No              | No             | Yes (`term.table`, `term.bar`) |
| Interactive tutorials | No        | No                | `go tour` (web) | No             | Yes (`forge learn`)            |
| Dual syntax           | No        | No                | No              | No             | Yes (classic + natural)        |
| Errors as values      | No        | No                | Yes             | Yes            | Yes (`Result`, `?`, `must`)    |
| Null safety           | No        | No                | No              | Yes (`Option`) | Yes (`Option`, no null)        |
| Semicolons required   | No        | Optional          | No              | Yes            | No                             |

> **Tip:** Forge is not trying to replace Rust or Go for systems programming. It is designed for application-layer work: web services, scripts, data pipelines, prototypes, and tooling. Think of it as the language you reach for when you want to build something that talks to the internet.

### Installing Forge

Forge is built with Rust, so you need the Rust toolchain installed first. If you don't have it, visit [https://rustup.rs](https://rustup.rs) and follow the instructions. You need Rust 1.85 or later.

Once Rust is ready, clone the repository and install:

```bash
git clone https://github.com/forge-lang/forge.git
cd forge
cargo install --path .
```

This compiles Forge from source and places the `forge` binary in your Cargo bin directory (typically `~/.cargo/bin/`). The build takes about 60–90 seconds on a modern machine.

Verify the installation:

```bash
forge version
```

You should see output like:

```
Forge 0.1.0
```

> **Tip:** If `forge` is not found after installation, make sure `~/.cargo/bin` is in your system `PATH`. Add `export PATH="$HOME/.cargo/bin:$PATH"` to your shell profile if needed.

### Your First Program

Create a file called `hello.fg` in your working directory:

```forge
let name = "World"
println("Hello, {name}!")
```

Run it:

```bash
forge run hello.fg
```

Output:

```
Hello, World!
```

That is the entire program. No `main` function, no imports, no boilerplate. Forge executes top-level statements in order, like a script.

Notice the string `"Hello, {name}!"`. The curly braces inside a double-quoted string perform _interpolation_ — the expression inside the braces is evaluated and its result is inserted into the string. This works with any expression, not just variable names.

Now try the natural-language syntax. Create `hello_natural.fg`:

```forge
set name to "World"
say "Hello, {name}!"
```

Run it with `forge run hello_natural.fg` and you get the same output. The `set ... to` syntax is equivalent to `let`, and `say` is equivalent to `println`. Both styles produce identical results.

### The REPL

Forge ships with an interactive Read-Eval-Print Loop. Start it by running `forge` with no arguments:

```bash
forge
```

You will see the Forge prompt:

```
forge>
```

Try some expressions:

```
forge> 2 + 2
4
forge> "hello" + " " + "world"
hello world
forge> let x = 42
forge> x * 2
84
```

The REPL supports multiline input. When you type an opening brace, Forge knows you are not done yet:

```
forge> fn greet(name) {
  ...     return "Hello, {name}!"
  ... }
forge> greet("Forge")
Hello, Forge!
```

The REPL also provides command history (press the up arrow to recall previous lines) and tab completion for keywords and built-in functions.

> **Tip:** Use the REPL to experiment with syntax as you read this book. It is the fastest way to test an idea.

### Interactive Tutorials

Forge includes 14 built-in interactive lessons that teach you the language step by step, right in your terminal. List them:

```bash
forge learn
```

Start a specific lesson:

```bash
forge learn 1
```

Each lesson explains a concept, shows you a code example, runs it, and displays the expected output. The lessons cover variables, functions, loops, error handling, HTTP, databases, and more. If you are new to programming, `forge learn` is the recommended starting point.

### Inline Evaluation

For quick one-liners, use the `-e` flag to evaluate an expression without creating a file:

```bash
forge -e 'say "Hello from the command line!"'
```

```bash
forge -e 'println(2 + 2)'
```

```bash
forge -e 'say math.sqrt(144)'
```

This is useful for quick calculations, testing syntax, and shell scripting.

### Editor Support

Forge provides a Language Server Protocol (LSP) server for editor integration:

```bash
forge lsp
```

For Visual Studio Code, a Forge extension is available that provides syntax highlighting, error diagnostics, and completion suggestions. Install it from the VS Code marketplace by searching for "Forge Language" or configure your editor to use the LSP server directly.

### Project Scaffolding

When you are ready to build something larger than a single file, use the `forge new` command to generate a project scaffold:

```bash
forge new my-app
```

This creates a directory structure with a main source file, a test file, and a configuration file — everything you need to start building.

### Try It Yourself

1. **Hello, You.** Create a file called `greeting.fg` that stores your name in a variable and prints a personalized greeting using string interpolation. Run it with `forge run`.

2. **REPL Explorer.** Open the Forge REPL and try these expressions: `math.pi`, `len("forge")`, `sort([5, 3, 1, 4, 2])`. What does each one return?

3. **Tutorial Time.** Run `forge learn 1` and complete the first interactive lesson. Then run `forge learn` to see the full list of available topics.

---

## Chapter 2: Variables and Types

Every program manipulates data, and every piece of data has a type. This chapter covers Forge's type system, how to declare variables, and the rules that govern mutability, type conversion, and truthiness. Understanding these fundamentals will make everything that follows in this book more intuitive.

### The Seven Fundamental Types

Forge has seven built-in types. Every value you create belongs to exactly one of them.

| Type     | Description                       | Example Literals               |
| -------- | --------------------------------- | ------------------------------ |
| `Int`    | 64-bit signed integer             | `42`, `-7`, `0`                |
| `Float`  | 64-bit floating point             | `3.14`, `-0.5`, `1.0`          |
| `String` | UTF-8 text                        | `"hello"`, `"Forge {version}"` |
| `Bool`   | Boolean truth value               | `true`, `false`                |
| `Array`  | Ordered collection                | `[1, 2, 3]`, `["a", "b"]`      |
| `Object` | Key-value map (insertion-ordered) | `{ name: "Alice", age: 30 }`   |
| `Null`   | Absence of value                  | `null`                         |

Let's look at each one:

```forge
let age = 30
let pi = 3.14159
let name = "Forge"
let active = true
let scores = [95, 87, 92]
let user = { name: "Alice", role: "engineer" }
let nothing = null
```

Forge also has two special wrapper types — `Result` and `Option` — which we will cover in detail in Chapter 8. For now, know that `Ok(value)` and `Err("message")` wrap results, and `Some(value)` and `None` wrap optional values.

### Declaring Variables

Forge provides two syntaxes for declaring variables: classic and natural.

**Classic syntax** uses `let`:

```forge
let city = "Portland"
let population = 652503
let elevation = 15.2
```

**Natural syntax** uses `set ... to`:

```forge
set city to "Portland"
set population to 652503
set elevation to 15.2
```

Both produce identical results. Use whichever reads better to you, or mix them within the same file.

### Immutable by Default

Variables in Forge are _immutable_ by default. Once assigned, their value cannot change:

```forge
let x = 10
x = 20
```

This program will produce an error: `cannot reassign immutable variable 'x'`. Immutability is a safety feature. It prevents accidental changes and makes code easier to reason about.

### Mutable Variables

When you need a variable that changes, declare it with `mut`:

```forge
let mut counter = 0
counter = counter + 1
counter = counter + 1
println("Counter: {counter}")
```

Output:

```
Counter: 2
```

In natural syntax, use `set mut` and `change`:

```forge
set mut counter to 0
change counter to counter + 1
change counter to counter + 1
say "Counter: {counter}"
```

The `change ... to` syntax is the natural-language equivalent of reassignment. It only works on variables declared with `mut`.

> **Tip:** Start with immutable variables. Only add `mut` when you genuinely need to change a value. This habit catches bugs early and communicates intent to anyone reading your code.

### Type Annotations

Forge uses _gradual typing_. Type annotations are optional — you can add them when you want clarity or extra safety, and omit them when the types are obvious:

```forge
let name: String = "Alice"
let age: Int = 30
let score: Float = 98.5
let active: Bool = true
```

Without annotations, Forge infers the types from the values:

```forge
let name = "Alice"
let age = 30
let score = 98.5
let active = true
```

Both versions behave identically. Annotations become more valuable in function signatures, where they document the expected inputs and outputs:

```forge
fn add(a: Int, b: Int) -> Int {
    return a + b
}
```

We will explore annotated functions in Chapter 6.

### String Interpolation

String interpolation is one of Forge's most frequently used features. Any expression inside curly braces within a double-quoted string is evaluated and converted to text:

```forge
let name = "Forge"
let version = 2
say "Welcome to {name} v{version}!"
```

Output:

```
Welcome to Forge v2!
```

Interpolation works with any expression, not just simple variables:

```forge
let x = 7
say "Seven squared is {x * x}"
say "The length of 'hello' is {len("hello")}"
say "Is 10 > 5? {10 > 5}"
```

Output:

```
Seven squared is 49
The length of 'hello' is 5
Is 10 > 5? true
```

You can nest function calls, arithmetic, and comparisons inside interpolation braces. This eliminates the need for string concatenation in most cases.

### Triple-Quoted Raw Strings

For strings that span multiple lines or contain characters you don't want to escape, use triple-quoted strings:

```forge
let sql = """SELECT * FROM users WHERE active = true"""
say sql
```

Triple-quoted strings preserve their content exactly as written. They are especially useful for SQL queries, regular expressions, and embedded data.

```forge
let html = """<div class="container">
    <h1>Hello, Forge!</h1>
    <p>This is raw HTML.</p>
</div>"""
say html
```

### Type Conversion

Forge provides built-in functions to convert between types:

```forge
let n = int("42")
say n + 8

let f = float("3.14")
say f * 2

let s = str(42)
say "The answer is " + s

say int("100") + int("200")
say float("1.5") + float("2.5")
```

Output:

```
50
6.28
The answer is 42
300
4.0
```

| Function       | Converts To | Example                  |
| -------------- | ----------- | ------------------------ |
| `int(value)`   | `Int`       | `int("42")` → `42`       |
| `float(value)` | `Float`     | `float("3.14")` → `3.14` |
| `str(value)`   | `String`    | `str(42)` → `"42"`       |

> **Tip:** `int()` and `float()` will produce an error if the input string cannot be parsed as a number. Always validate user input before converting.

### Type Inspection

You can check the type of any value at runtime:

```forge
say typeof(42)
say typeof("hello")
say typeof(true)
say typeof([1, 2, 3])
say typeof({ name: "Alice" })
say typeof(null)
```

Output:

```
Int
String
Bool
Array
Object
Null
```

The `typeof()` function returns a string describing the type. This is useful for debugging, validation, and writing functions that handle multiple types.

The `type()` function is an alias for `typeof()`:

```forge
let value = 3.14
if type(value) == "Float" {
    say "It's a floating-point number"
}
```

### Truthiness

Forge evaluates values as "truthy" or "falsy" when used in boolean contexts (like `if` conditions). The rules are straightforward:

| Value               | Truthy? |
| ------------------- | ------- |
| `false`             | Falsy   |
| `null`              | Falsy   |
| `0` (integer zero)  | Falsy   |
| `0.0` (float zero)  | Falsy   |
| `""` (empty string) | Falsy   |
| `[]` (empty array)  | Falsy   |
| Everything else     | Truthy  |

```forge
if "hello" {
    say "Non-empty strings are truthy"
}

if 0 {
    say "This won't print"
} else {
    say "Zero is falsy"
}

if [1, 2, 3] {
    say "Non-empty arrays are truthy"
}

if [] {
    say "This won't print"
} else {
    say "Empty arrays are falsy"
}
```

Output:

```
Non-empty strings are truthy
Zero is falsy
Non-empty arrays are truthy
Empty arrays are falsy
```

> **Tip:** If you want explicit boolean checks rather than relying on truthiness, compare directly: `if len(items) > 0 { ... }` instead of `if items { ... }`. Explicit comparisons are clearer, especially when other developers will read your code.

### Variable Types Cheat Sheet

| Declaration      | Syntax Style     | Mutable? | Example                     |
| ---------------- | ---------------- | -------- | --------------------------- |
| `let x = 5`      | Classic          | No       | `let name = "Alice"`        |
| `let mut x = 5`  | Classic          | Yes      | `let mut count = 0`         |
| `set x to 5`     | Natural          | No       | `set name to "Alice"`       |
| `set mut x to 5` | Natural          | Yes      | `set mut count to 0`        |
| `x = 10`         | Classic reassign | —        | `count = count + 1`         |
| `change x to 10` | Natural reassign | —        | `change count to count + 1` |
| `let x: Int = 5` | Annotated        | No       | `let age: Int = 30`         |

### Try It Yourself

1. **Type Explorer.** Write a program that creates one variable of each of the seven types and prints both the value and its type using `typeof()`. For example: `say "42 is a {typeof(42)}"`.

2. **Mutability Practice.** Declare a mutable variable called `balance` starting at `1000`. Subtract `250` three times using reassignment, then print the final balance. Try doing it once with classic syntax and once with natural syntax.

3. **Interpolation Challenge.** Write a program that stores a person's first name, last name, and birth year in variables, then prints a single sentence like: `"Alice Johnson was born in 1990 and is 36 years old."` — computing the age from the birth year using an expression inside the interpolation braces.

---

## Chapter 3: Operators and Expressions

Operators are the verbs of a programming language — they describe what to _do_ with your data. This chapter covers every operator Forge supports, from basic arithmetic to compound assignment, along with the rules that govern how expressions are evaluated.

### Arithmetic Operators

Forge supports the standard arithmetic operators:

| Operator | Operation          | Example | Result |
| -------- | ------------------ | ------- | ------ |
| `+`      | Addition           | `7 + 3` | `10`   |
| `-`      | Subtraction        | `7 - 3` | `4`    |
| `*`      | Multiplication     | `7 * 3` | `21`   |
| `/`      | Division           | `7 / 3` | `2`    |
| `%`      | Modulo (remainder) | `7 % 3` | `1`    |

```forge
say 10 + 3
say 10 - 3
say 10 * 3
say 10 / 3
say 10 % 3
```

Output:

```
13
7
30
3
1
```

### Integer vs. Float Division

When both operands are integers, division produces an integer result (truncating any remainder):

```forge
say 7 / 2
say 10 / 3
```

Output:

```
3
3
```

When either operand is a float, the result is a float:

```forge
say 7.0 / 2
say 7 / 2.0
say 10.0 / 3.0
```

Output:

```
3.5
3.5
3.3333333333333335
```

This behavior matches most systems languages. If you want floating-point division with integer operands, convert one to a float first:

```forge
let a = 7
let b = 2
say float(a) / float(b)
```

Output:

```
3.5
```

> **Tip:** Division by zero with integers causes a runtime error. Always validate divisors when working with user input or computed values.

### Compound Assignment Operators

Forge supports shorthand operators that combine arithmetic with assignment. These only work on mutable variables:

```forge
let mut x = 10
x += 5
say x

x -= 3
say x

x *= 2
say x

x /= 4
say x
```

Output:

```
15
12
24
6
```

| Operator | Equivalent To   | Example  |
| -------- | --------------- | -------- |
| `+=`     | `x = x + value` | `x += 5` |
| `-=`     | `x = x - value` | `x -= 3` |
| `*=`     | `x = x * value` | `x *= 2` |
| `/=`     | `x = x / value` | `x /= 4` |

Compound assignment is syntactic sugar — `x += 5` means exactly the same thing as `x = x + 5`. Use whichever is clearer in context.

### Comparison Operators

Comparison operators return a boolean value (`true` or `false`):

| Operator | Meaning               | Example  | Result  |
| -------- | --------------------- | -------- | ------- |
| `==`     | Equal to              | `5 == 5` | `true`  |
| `!=`     | Not equal to          | `5 != 3` | `true`  |
| `<`      | Less than             | `3 < 5`  | `true`  |
| `>`      | Greater than          | `5 > 3`  | `true`  |
| `<=`     | Less than or equal    | `5 <= 5` | `true`  |
| `>=`     | Greater than or equal | `5 >= 6` | `false` |

```forge
let a = 10
let b = 20

say a == b
say a != b
say a < b
say a > b
say a <= 10
say b >= 20
```

Output:

```
false
true
true
false
true
true
```

Strings are compared lexicographically (dictionary order):

```forge
say "apple" < "banana"
say "zebra" > "aardvark"
say "hello" == "hello"
```

Output:

```
true
true
true
```

### Logical Operators

Logical operators combine boolean values:

| Operator | Meaning     | Example           | Result  |
| -------- | ----------- | ----------------- | ------- |
| `&&`     | Logical AND | `true && false`   | `false` |
| `\|\|`   | Logical OR  | `true \|\| false` | `true`  |
| `!`      | Logical NOT | `!true`           | `false` |

```forge
let age = 25
let has_license = true

if age >= 16 && has_license {
    say "You can drive"
}

let is_weekend = false
let is_holiday = true

if is_weekend || is_holiday {
    say "No work today!"
}

let raining = false
if !raining {
    say "Go outside"
}
```

Output:

```
You can drive
No work today!
Go outside
```

### String Concatenation

The `+` operator concatenates strings when both operands are strings:

```forge
let first = "Hello"
let second = "World"
let greeting = first + ", " + second + "!"
say greeting
```

Output:

```
Hello, World!
```

In most cases, string interpolation is cleaner than concatenation:

```forge
let first = "Hello"
let second = "World"
say "{first}, {second}!"
```

Both approaches produce the same result. Prefer interpolation for readability; use concatenation when building strings incrementally.

### Negation

The unary minus operator negates a number:

```forge
let x = 42
say -x

let temperature = -15
say temperature
say -temperature
```

Output:

```
-42
-15
15
```

### Operator Precedence

When an expression contains multiple operators, Forge evaluates them in a specific order. Higher-precedence operators bind more tightly:

| Precedence | Operators                             | Associativity |
| ---------- | ------------------------------------- | ------------- |
| Highest    | `!` (unary NOT), `-` (unary negation) | Right-to-left |
|            | `*`, `/`, `%`                         | Left-to-right |
|            | `+`, `-`                              | Left-to-right |
|            | `<`, `>`, `<=`, `>=`                  | Left-to-right |
|            | `==`, `!=`                            | Left-to-right |
|            | `&&`                                  | Left-to-right |
| Lowest     | `\|\|`                                | Left-to-right |

```forge
say 2 + 3 * 4
say (2 + 3) * 4
```

Output:

```
14
20
```

Multiplication binds more tightly than addition, so `2 + 3 * 4` is evaluated as `2 + (3 * 4)`. Use parentheses to override the default order when needed.

```forge
let result = 10 > 5 && 3 < 7
say result
```

Output:

```
true
```

Here, the comparisons (`10 > 5` and `3 < 7`) are evaluated first, then `&&` combines the two boolean results.

> **Tip:** When in doubt, add parentheses. They cost nothing at runtime and make your intent unambiguous to both the computer and the human reading your code.

### Expression Evaluation Order

Forge evaluates expressions left to right within the same precedence level. This matters most with function calls that have side effects:

```forge
let a = 5
let b = 3
let c = 2

let result = a + b * c - a / c
say result
```

Step by step:

1. `b * c` → `6` (multiplication first)
2. `a / c` → `2` (division, same precedence as multiplication, left to right)
3. `a + 6` → `11` (addition)
4. `11 - 2` → `9` (subtraction)

Output:

```
9
```

### Try It Yourself

1. **Calculator.** Write a program that stores two numbers in variables and prints the result of all five arithmetic operations (`+`, `-`, `*`, `/`, `%`) on them, each on its own line. Test with both integer and float values.

2. **Compound Assignment Chain.** Start with `let mut x = 100`. Apply `+= 50`, then `*= 2`, then `-= 75`, then `/= 5`. Print `x` after each step. What is the final value?

3. **Precedence Puzzle.** Without running the code, predict the output of each expression. Then verify in the REPL.
   - `2 + 3 * 4 - 1`
   - `(2 + 3) * (4 - 1)`
   - `10 / 2 + 3 * 4 - 1`
   - `true || false && false`

---

## Chapter 4: Control Flow

Programs that run in a straight line from top to bottom are useful, but limited. Real programs make decisions: they choose one path over another based on conditions. This chapter covers every branching construct in Forge, from basic `if/else` to the powerful `when` guard expression.

### The if Statement

The `if` statement is the most fundamental control flow construct. It executes a block of code only when a condition is true:

```forge
let temperature = 35

if temperature > 30 {
    say "It's hot outside!"
}
```

Output:

```
It's hot outside!
```

The condition must evaluate to a truthy value (see the truthiness table in Chapter 2). The braces around the body are required — Forge does not support braceless `if` statements.

### if/else

Add an `else` branch to handle the case when the condition is false:

```forge
let age = 15

if age >= 18 {
    say "You are an adult"
} else {
    say "You are a minor"
}
```

Output:

```
You are a minor
```

### else if Chains

Chain multiple conditions with `else if`:

```forge
let score = 85

if score >= 90 {
    say "Grade: A"
} else if score >= 80 {
    say "Grade: B"
} else if score >= 70 {
    say "Grade: C"
} else if score >= 60 {
    say "Grade: D"
} else {
    say "Grade: F"
}
```

Output:

```
Grade: B
```

Forge evaluates conditions from top to bottom and executes the first branch whose condition is true. Once a branch executes, the remaining branches are skipped.

### otherwise and nah

Forge provides two natural-language alternatives to `else`:

**`otherwise`** reads like prose:

```forge
let ready = false

if ready {
    say "Let's go!"
} otherwise {
    say "Not yet"
}
```

**`nah`** is informal and fun:

```forge
let has_coffee = true

if has_coffee {
    say "Productive morning"
} nah {
    say "Need coffee first"
}
```

Both `otherwise` and `nah` are exact synonyms for `else`. Use them to make your code read the way you think. You can also use `otherwise if` in chains:

```forge
set score to 85

if score > 90 {
    say "Grade: A"
} otherwise if score > 80 {
    say "Grade: B"
} otherwise {
    say "Grade: C"
}
```

Output:

```
Grade: B
```

### if as an Expression

In Forge, `if` can be used as an expression that returns a value. The last expression in each branch becomes the result:

```forge
let age = 20
let status = if age >= 18 { "adult" } else { "minor" }
say status
```

Output:

```
adult
```

This is Forge's equivalent of the ternary operator found in other languages. It eliminates the need for a separate `? :` syntax:

```forge
let temperature = 25
let advice = if temperature > 30 {
    "Stay hydrated"
} else if temperature > 20 {
    "Perfect weather"
} else {
    "Bring a jacket"
}
say advice
```

Output:

```
Perfect weather
```

> **Tip:** When using `if` as an expression, always include an `else` branch. Without it, the expression would have no value when the condition is false, which could lead to unexpected `null` results.

### when Guards

The `when` expression is unique to Forge. It provides a concise way to match a value against multiple conditions:

```forge
set age to 25

when age {
    < 13 -> "kid"
    < 20 -> "teen"
    < 65 -> "adult"
    else -> "senior"
}
```

Think of `when` as a multi-way conditional that tests a single subject against a series of comparison operators. Each arm uses `->` to separate the condition from the result.

Here is a more practical example:

```forge
let score = 87

let grade = when score {
    >= 90 -> "A"
    >= 80 -> "B"
    >= 70 -> "C"
    >= 60 -> "D"
    else -> "F"
}
say "Your grade: {grade}"
```

Output:

```
Your grade: B
```

The `when` construct is particularly useful for categorizing numeric values:

```forge
let http_status = 404

when http_status {
    < 200 -> say "Informational"
    < 300 -> say "Success"
    < 400 -> say "Redirect"
    < 500 -> say "Client Error"
    else -> say "Server Error"
}
```

Output:

```
Client Error
```

> **Tip:** `when` evaluates arms from top to bottom and stops at the first match, just like `else if` chains. Order matters: put the most specific conditions first.

### Nested Conditionals

You can nest `if` statements inside other `if` statements for complex logic:

```forge
let age = 25
let has_id = true

if age >= 21 {
    if has_id {
        say "Welcome to the bar"
    } else {
        say "Please show your ID"
    }
} else {
    say "You must be 21 or older"
}
```

Output:

```
Welcome to the bar
```

While nesting works, deep nesting makes code hard to read. Consider flattening with `&&`:

```forge
let age = 25
let has_id = true

if age >= 21 && has_id {
    say "Welcome to the bar"
} else if age >= 21 {
    say "Please show your ID"
} else {
    say "You must be 21 or older"
}
```

This version communicates the same logic with less indentation.

### Boolean Short-Circuit Evaluation

Forge uses _short-circuit evaluation_ for `&&` and `||`. This means:

- `&&` stops evaluating if the left side is false (the overall result is already determined)
- `||` stops evaluating if the left side is true

```forge
let x = 0

if x != 0 && 10 / x > 2 {
    say "This won't cause a division by zero"
}
```

Because `x != 0` is false, the right side (`10 / x > 2`) is never evaluated, which prevents a division-by-zero error. Short-circuit evaluation is not just an optimization — it is a safety feature.

```forge
fn expensive_check() {
    say "This function was called"
    return true
}

if true || expensive_check() {
    say "Short-circuited"
}
```

Output:

```
Short-circuited
```

The `expensive_check()` function is never called because the left side of `||` is already `true`.

### Combining Conditions

Complex business logic often requires combining multiple conditions. Use parentheses to group and clarify:

```forge
let age = 30
let is_student = true
let income = 25000

if (age < 26 || is_student) && income < 30000 {
    say "Eligible for discount"
} else {
    say "Standard pricing"
}
```

Output:

```
Eligible for discount
```

Without parentheses, operator precedence would evaluate `&&` before `||`, potentially changing the meaning. When combining logical operators, explicit parentheses prevent subtle bugs.

### Try It Yourself

1. **Grade Calculator.** Write a program that assigns a letter grade based on a numeric score. Use `when` guards. Include grades A+ (97+), A (93+), A- (90+), B+ (87+), B (83+), B- (80+), and so on down to F (below 60).

2. **Leap Year.** A year is a leap year if it is divisible by 4, except for years divisible by 100, unless they are also divisible by 400. Write a program that determines whether a given year is a leap year using only `if/else` (no functions yet). Test with 2000, 1900, 2024, and 2023.

3. **FizzBuzz (Conditional).** Write a program that checks a single number: if it is divisible by both 3 and 5, print "FizzBuzz"; if divisible by 3 only, print "Fizz"; if divisible by 5 only, print "Buzz"; otherwise, print the number. (We will loop through many numbers in the next chapter.)

---

## Chapter 5: Loops and Iteration

Loops let you repeat a block of code. Whether you are processing every item in a list, waiting for a condition to change, or counting from one to a million, loops are the mechanism. Forge provides five loop constructs, each suited to a different kind of repetition.

### for/in Loops

The `for/in` loop iterates over each element in an array:

```forge
let fruits = ["apple", "banana", "cherry"]

for fruit in fruits {
    say "I like {fruit}"
}
```

Output:

```
I like apple
I like banana
I like cherry
```

The loop variable (`fruit`) is automatically created for each iteration. It is scoped to the loop body — it does not exist outside the loop.

### for each (Natural Syntax)

The natural-language version adds the word `each` for readability:

```forge
set colors to ["red", "green", "blue"]

for each color in colors {
    say "Color: {color}"
}
```

Output:

```
Color: red
Color: green
Color: blue
```

`for each` and `for` are identical in behavior. The `each` keyword is optional syntactic sugar.

### Iterating Over Objects

When iterating over an object, you can bind both the key and the value:

```forge
let user = { name: "Alice", age: 30, role: "engineer" }

for key, value in user {
    say "{key}: {value}"
}
```

Output:

```
name: Alice
age: 30
role: engineer
```

Objects in Forge are insertion-ordered, so the loop visits keys in the order they were defined. This makes object iteration predictable and useful for building reports, generating output, or transforming data.

### while Loops

The `while` loop repeats as long as a condition is true:

```forge
let mut count = 0

while count < 5 {
    say "Count: {count}"
    count = count + 1
}
```

Output:

```
Count: 0
Count: 1
Count: 2
Count: 3
Count: 4
```

A `while` loop is the right choice when you don't know in advance how many iterations you need:

```forge
let mut n = 1
while n < 1000 {
    n = n * 2
}
say "First power of 2 >= 1000: {n}"
```

Output:

```
First power of 2 >= 1000: 1024
```

> **Tip:** Make sure your `while` condition will eventually become false. A condition that never changes creates an infinite loop that hangs your program.

### repeat N times

When you know exactly how many times to repeat, `repeat` is the cleanest syntax:

```forge
repeat 5 times {
    say "Hello!"
}
```

Output:

```
Hello!
Hello!
Hello!
Hello!
Hello!
```

`repeat` is unique to Forge. It reads like a natural instruction — "repeat 5 times" — which makes it ideal for simple counted repetition without the ceremony of a counter variable:

```forge
set mut stars to ""
repeat 10 times {
    change stars to stars + "*"
}
say stars
```

Output:

```
**********
```

### loop (Infinite Loop with break)

The `loop` construct creates an infinite loop. You must use `break` to exit:

```forge
let mut i = 0
loop {
    if i >= 5 {
        break
    }
    say "i = {i}"
    i = i + 1
}
```

Output:

```
i = 0
i = 1
i = 2
i = 3
i = 4
```

`loop` is useful when the exit condition is complex or occurs in the middle of the loop body rather than at the top:

```forge
let mut sum = 0
let mut n = 1

loop {
    sum = sum + n
    if sum > 100 {
        say "Stopped at n = {n}, sum = {sum}"
        break
    }
    n = n + 1
}
```

Output:

```
Stopped at n = 14, sum = 105
```

### break and continue

The `break` keyword exits the innermost loop immediately. The `continue` keyword skips the rest of the current iteration and moves to the next one:

```forge
let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

for n in numbers {
    if n % 2 == 0 {
        continue
    }
    say n
}
```

Output:

```
1
3
5
7
9
```

Here, `continue` skips even numbers, so only odd numbers are printed.

```forge
let items = ["apple", "banana", "STOP", "cherry", "date"]

for item in items {
    if item == "STOP" {
        say "Found stop signal, exiting loop"
        break
    }
    say "Processing: {item}"
}
```

Output:

```
Processing: apple
Processing: banana
Found stop signal, exiting loop
```

### range() for Numeric Ranges

The `range()` function generates an array of sequential integers, which you can iterate over:

```forge
for i in range(5) {
    say i
}
```

Output:

```
0
1
2
3
4
```

`range(n)` produces integers from 0 to n-1. You can also specify a start and end:

```forge
for i in range(1, 6) {
    say i
}
```

Output:

```
1
2
3
4
5
```

Use `range()` whenever you need a numeric counter in a `for` loop:

```forge
let mut sum = 0
for i in range(1, 101) {
    sum += i
}
say "Sum of 1 to 100: {sum}"
```

Output:

```
Sum of 1 to 100: 5050
```

### enumerate() for Indexed Iteration

When you need both the index and the value, use `enumerate()`:

```forge
let languages = ["Rust", "Forge", "Go", "Python"]

for i, lang in enumerate(languages) {
    say "{i}: {lang}"
}
```

Output:

```
0: Rust
1: Forge
2: Go
3: Python
```

`enumerate()` wraps each element with its zero-based index, giving you both pieces of information without maintaining a separate counter.

### Nested Loops

Loops can be nested inside other loops:

```forge
for i in range(1, 4) {
    for j in range(1, 4) {
        let product = i * j
        print("{product}\t")
    }
    println("")
}
```

Output:

```
1	2	3
2	4	6
3	6	9
```

When using `break` or `continue` in nested loops, they apply to the _innermost_ enclosing loop:

```forge
for i in range(3) {
    for j in range(3) {
        if j == 1 {
            break
        }
        say "i={i}, j={j}"
    }
}
```

Output:

```
i=0, j=0
i=1, j=0
i=2, j=0
```

The `break` exits only the inner loop. The outer loop continues to the next iteration.

### Choosing the Right Loop

| Scenario                           | Best Loop                      |
| ---------------------------------- | ------------------------------ |
| Process every item in a collection | `for item in array`            |
| Fixed number of repetitions        | `repeat N times`               |
| Repeat until a condition changes   | `while condition`              |
| Complex exit logic                 | `loop` with `break`            |
| Count through numbers              | `for i in range(n)`            |
| Need index + value                 | `for i, v in enumerate(array)` |

### Try It Yourself

1. **Multiplication Table.** Write a program that prints a 10x10 multiplication table using nested `for` loops with `range()`.

2. **FizzBuzz Complete.** Using a `for` loop over `range(1, 101)`, print "Fizz" for multiples of 3, "Buzz" for multiples of 5, "FizzBuzz" for multiples of both, and the number itself otherwise.

3. **Search and Stop.** Create an array of 10 city names. Use a `for` loop with `break` to search for a specific city. Print "Found [city] at index [i]" using `enumerate()` and stop searching once found. If the city is not in the list, print "Not found."

---

## Chapter 6: Functions and Closures

Functions are the primary unit of code organization in Forge. They let you name a block of code, give it parameters, and call it from anywhere. This chapter covers function definition, closures, higher-order functions, recursion, and decorators.

### Defining Functions

Forge provides two syntaxes for defining functions:

**Classic syntax** uses `fn`:

```forge
fn greet(name) {
    println("Hello, {name}!")
}

greet("World")
```

**Natural syntax** uses `define`:

```forge
define greet(name) {
    say "Hello, {name}!"
}

greet("World")
```

Both produce identical functions. The function name, parameter list, and body are the same — only the keyword differs.

### Parameters and Return Values

Functions accept zero or more parameters and optionally return a value:

```forge
fn add(a, b) {
    return a + b
}

let result = add(3, 4)
say result
```

Output:

```
7
```

If a function has no explicit `return` statement, it returns `null`:

```forge
fn log_message(msg) {
    println("[LOG] {msg}")
}

let result = log_message("server started")
say typeof(result)
```

Output:

```
[LOG] server started
Null
```

Functions can return early:

```forge
fn classify(n) {
    if n < 0 {
        return "negative"
    }
    if n == 0 {
        return "zero"
    }
    return "positive"
}

say classify(-5)
say classify(0)
say classify(42)
```

Output:

```
negative
zero
positive
```

### Type-Annotated Functions

Add type annotations to parameters and return values for documentation and safety:

```forge
fn add(a: Int, b: Int) -> Int {
    return a + b
}

fn format_price(amount: Float) -> String {
    return "${amount}"
}

say add(10, 20)
say format_price(9.99)
```

Output:

```
30
$9.99
```

Annotations are optional. Use them when the function's purpose is not obvious from its name and parameter names alone. They are especially valuable in public APIs and shared codebases.

### Multiple Return Values

Forge functions can only return a single value, but you can use arrays or objects to return multiple pieces of data:

```forge
fn min_max(numbers) {
    let mut lo = numbers[0]
    let mut hi = numbers[0]
    for n in numbers {
        if n < lo { lo = n }
        if n > hi { hi = n }
    }
    return { min: lo, max: hi }
}

let result = min_max([4, 7, 1, 9, 3])
say "Min: {result.min}, Max: {result.max}"
```

Output:

```
Min: 1, Max: 9
```

Using objects for multiple return values is idiomatic in Forge because the field names document what each value represents.

### Closures

A closure is an anonymous function that captures variables from its surrounding scope:

```forge
let double = fn(x) { return x * 2 }
say double(21)
```

Output:

```
42
```

Closures can capture variables from the enclosing function:

```forge
fn make_adder(n) {
    return fn(x) {
        return x + n
    }
}

let add5 = make_adder(5)
let add10 = make_adder(10)

say add5(3)
say add10(3)
```

Output:

```
8
13
```

Each call to `make_adder` creates a new closure that remembers the value of `n`. The closure "closes over" the variable — hence the name. This is a powerful pattern for creating specialized functions from a general template.

> **Tip:** Think of a closure as a function bundled with a snapshot of its environment. The captured variables travel with the closure wherever it goes.

### Higher-Order Functions

A higher-order function is a function that takes another function as a parameter or returns one. We just saw an example with `make_adder` (which returns a function). Here is one that accepts a function:

```forge
fn apply(f, value) {
    return f(value)
}

fn square(x) { return x * x }
fn negate(x) { return -x }

say apply(square, 7)
say apply(negate, 42)
```

Output:

```
49
-42
```

Higher-order functions are the foundation of functional programming in Forge. The built-in `map`, `filter`, and `reduce` functions (covered in Chapter 7) are all higher-order functions.

```forge
let numbers = [1, 2, 3, 4, 5]

let doubled = map(numbers, fn(x) { return x * 2 })
say doubled

let evens = filter(numbers, fn(x) { return x % 2 == 0 })
say evens
```

Output:

```
[2, 4, 6, 8, 10]
[2, 4]
```

### Recursion

A recursive function calls itself. It must have a base case that stops the recursion and a recursive case that makes progress toward the base case:

```forge
fn factorial(n) {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

say factorial(5)
say factorial(10)
```

Output:

```
120
3628800
```

Here is the classic Fibonacci sequence:

```forge
fn fib(n) {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

for i in range(10) {
    let f = fib(i)
    println("fib({i}) = {f}")
}
```

Output:

```
fib(0) = 0
fib(1) = 1
fib(2) = 1
fib(3) = 2
fib(4) = 3
fib(5) = 5
fib(6) = 8
fib(7) = 13
fib(8) = 21
fib(9) = 34
```

> **Tip:** The naive Fibonacci implementation has exponential time complexity. For production code, use memoization or an iterative approach. Recursion is a teaching tool here, not a performance recommendation.

### Iterative Fibonacci (for comparison)

```forge
fn fib_fast(n) {
    if n <= 1 { return n }
    let mut a = 0
    let mut b = 1
    for i in range(2, n + 1) {
        let temp = a + b
        a = b
        b = temp
    }
    return b
}

say fib_fast(50)
```

### Decorators

Forge supports decorators — annotations prefixed with `@` that modify or categorize functions. The most common decorators are for testing and HTTP routing:

**Test decorator:**

```forge
@test
fn test_addition() {
    assert_eq(2 + 2, 4)
}

@test
define test_string_length() {
    assert_eq(len("forge"), 5)
}
```

Run tests with `forge test`.

**HTTP decorators:**

```forge
@server(port: 8080)

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}

@post("/echo")
fn echo(body: Json) -> Json {
    return body
}
```

Decorators are declarative metadata. They tell Forge _what_ the function is used for without cluttering the function body with framework-specific code. We will explore HTTP decorators in detail in a later part of this book.

### Functions as First-Class Values

In Forge, functions are values. You can store them in variables, put them in arrays, pass them as arguments, and return them from other functions:

```forge
fn greet(name) {
    return "Hello, {name}!"
}

let my_func = greet
say my_func("Forge")
```

Output:

```
Hello, Forge!
```

Storing functions in data structures:

```forge
fn add(a, b) { return a + b }
fn sub(a, b) { return a - b }
fn mul(a, b) { return a * b }

let operations = [add, sub, mul]
let names = ["add", "sub", "mul"]

for i, op in enumerate(operations) {
    let result = op(10, 3)
    let name = names[i]
    say "{name}(10, 3) = {result}"
}
```

Output:

```
add(10, 3) = 13
sub(10, 3) = 7
mul(10, 3) = 30
```

### Compact Function Bodies

For simple functions, you can write the body on a single line:

```forge
fn double(x) { return x * 2 }
fn is_positive(x) { return x > 0 }
fn identity(x) { return x }
```

This keeps utility functions compact without sacrificing readability.

### Try It Yourself

1. **Temperature Converter.** Write two functions: `celsius_to_fahrenheit(c)` and `fahrenheit_to_celsius(f)`. Use the formulas F = C × 9/5 + 32 and C = (F - 32) × 5/9. Test with `0°C`, `100°C`, `32°F`, and `212°F`.

2. **Closure Counter.** Write a function `make_counter()` that returns a closure. Each time the closure is called, it should return the next integer starting from 1. Calling the returned closure four times should produce 1, 2, 3, 4. (Hint: the closure captures a mutable variable.)

3. **Apply Twice.** Write a function `apply_twice(f, x)` that applies function `f` to `x` two times — i.e., it computes `f(f(x))`. Test it with a function that adds 3 and an initial value of 7 (expected result: 13). Then test it with a function that doubles its input and an initial value of 5 (expected result: 20).

---

## Chapter 7: Collections

Collections are data structures that hold multiple values. Forge has two primary collection types: arrays (ordered lists) and objects (key-value maps). This chapter covers both in depth, including the functional operations that make collection processing concise and expressive.

### Arrays

An array is an ordered, zero-indexed sequence of values:

```forge
let numbers = [1, 2, 3, 4, 5]
let names = ["Alice", "Bob", "Charlie"]
let mixed = [1, "hello", true, 3.14]
let empty = []
```

Arrays can hold values of any type, including other arrays:

```forge
let matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
say matrix[1][2]
```

Output:

```
6
```

### Array Access and Modification

Access elements by index (zero-based):

```forge
let fruits = ["apple", "banana", "cherry"]
say fruits[0]
say fruits[1]
say fruits[2]
```

Output:

```
apple
banana
cherry
```

Modify elements by assigning to an index:

```forge
let mut colors = ["red", "green", "blue"]
colors[1] = "yellow"
say colors
```

Output:

```
[red, yellow, blue]
```

### Array Built-in Operations

Forge provides a rich set of built-in functions for working with arrays:

```forge
let mut items = [3, 1, 4, 1, 5, 9, 2, 6]

say len(items)
say sort(items)
say reverse(items)
say contains(items, 5)
say contains(items, 99)
```

Output:

```
8
[1, 1, 2, 3, 4, 5, 6, 9]
[6, 2, 9, 5, 1, 4, 1, 3]
true
false
```

**Mutating operations** — `push` and `pop`:

```forge
let mut stack = [1, 2, 3]
push(stack, 4)
say stack

let top = pop(stack)
say "Popped: {top}"
say stack
```

Output:

```
[1, 2, 3, 4]
Popped: 4
[1, 2, 3]
```

Here is a complete reference of array operations:

| Function             | Description               | Example                        |
| -------------------- | ------------------------- | ------------------------------ |
| `len(arr)`           | Number of elements        | `len([1,2,3])` → `3`           |
| `push(arr, val)`     | Add to end (mutates)      | `push(arr, 4)`                 |
| `pop(arr)`           | Remove and return last    | `pop(arr)` → last element      |
| `sort(arr)`          | Return sorted copy        | `sort([3,1,2])` → `[1,2,3]`    |
| `reverse(arr)`       | Return reversed copy      | `reverse([1,2,3])` → `[3,2,1]` |
| `contains(arr, val)` | Check membership          | `contains([1,2], 2)` → `true`  |
| `range(n)`           | Generate `[0..n-1]`       | `range(3)` → `[0,1,2]`         |
| `enumerate(arr)`     | Pairs of `(index, value)` | See Chapter 5                  |

### map — Transform Every Element

The `map` function applies a transformation function to every element and returns a new array:

```forge
let numbers = [1, 2, 3, 4, 5]
let doubled = map(numbers, fn(x) { return x * 2 })
say doubled
```

Output:

```
[2, 4, 6, 8, 10]
```

```forge
let names = ["alice", "bob", "charlie"]
let lengths = map(names, fn(name) { return len(name) })
say lengths
```

Output:

```
[5, 3, 7]
```

`map` never modifies the original array. It always returns a new one.

### filter — Select Matching Elements

The `filter` function keeps only elements for which a predicate returns true:

```forge
let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
let evens = filter(numbers, fn(x) { return x % 2 == 0 })
say evens
```

Output:

```
[2, 4, 6, 8, 10]
```

```forge
let words = ["hello", "hi", "hey", "howdy", "greetings"]
let short_words = filter(words, fn(w) { return len(w) <= 3 })
say short_words
```

Output:

```
[hi, hey]
```

### reduce — Combine Into a Single Value

The `reduce` function collapses an array into a single value by applying a function cumulatively:

```forge
let numbers = [1, 2, 3, 4, 5]
let sum = reduce(numbers, 0, fn(acc, x) { return acc + x })
say sum
```

Output:

```
15
```

The second argument (`0`) is the initial value of the accumulator. The function receives the accumulator and the current element, and returns the new accumulator value.

```forge
let numbers = [3, 7, 2, 9, 4]
let maximum = reduce(numbers, numbers[0], fn(max, x) {
    if x > max { return x }
    return max
})
say "Maximum: {maximum}"
```

Output:

```
Maximum: 9
```

> **Tip:** Think of `reduce` as "folding" a list into a single value. The accumulator carries the running result, and each element updates it. This pattern is extraordinarily powerful — almost any array processing can be expressed as a `reduce`.

### Chaining Functional Operations

The real power of `map`, `filter`, and `reduce` emerges when you chain them together:

```forge
let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

let evens = filter(numbers, fn(x) { return x % 2 == 0 })
let doubled = map(evens, fn(x) { return x * 2 })
let total = reduce(doubled, 0, fn(acc, x) { return acc + x })

say "Sum of doubled evens: {total}"
```

Output:

```
Sum of doubled evens: 60
```

Step by step:

1. `filter` keeps `[2, 4, 6, 8, 10]`
2. `map` produces `[4, 8, 12, 16, 20]`
3. `reduce` sums to `60`

This is a data pipeline — each operation transforms the data and passes it to the next.

### Method Chaining

These functional operations also work as methods. You can call them directly on arrays and objects for a fluent, chainable style:

```forge
let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

let result = numbers.filter(fn(x) { return x % 2 == 0 }).map(fn(x) { return x * 2 })
say result
```

Output:

```
[4, 8, 12, 16, 20]
```

**`array.find(fn)`** — find first element matching the predicate:

```forge
let nums = [3, 7, 2, 9, 4]
let first_big = nums.find(fn(x) { return x > 5 })
say first_big
```

Output:

```
7
```

Object helper functions work as methods too:

```forge
let user = { name: "Alice", age: 30, email: "alice@example.com" }

say user.pick(["name", "email"])
say user.omit(["age"])
say user.merge({ role: "engineer" })
say user.has_key("name")
say user.get("age", 0)
```

Output:

```
{name: Alice, email: alice@example.com}
{name: Alice, email: alice@example.com}
{name: Alice, age: 30, email: alice@example.com, role: engineer}
true
30
```

**`flat_map(array, fn)`** — map and flatten in one step. The function must return an array for each element; the results are concatenated:

```forge
let words = ["hello", "world"]
let letters = flat_map(words, fn(w) { return split(w, "") })
say letters
```

Output:

```
[h, e, l, l, o, w, o, r, l, d]
```

### Objects

An object is an insertion-ordered collection of key-value pairs, similar to JSON objects:

```forge
let user = {
    name: "Alice",
    age: 30,
    role: "engineer"
}

say user.name
say user.age
say user.role
```

Output:

```
Alice
30
engineer
```

Objects use the syntax `{ key: value, key: value }`. Keys are unquoted identifiers; values can be any Forge type.

### Nested Objects

Objects can contain other objects:

```forge
let company = {
    name: "Acme Corp",
    address: {
        street: "123 Main St",
        city: "Portland",
        state: "OR"
    },
    founded: 2020
}

say company.name
say company.address.city
say company.address.state
```

Output:

```
Acme Corp
Portland
OR
```

### Object Operations

```forge
let config = { host: "localhost", port: 8080, debug: true }

say keys(config)
say values(config)
say len(config)
```

Output:

```
[host, port, debug]
[localhost, 8080, true]
3
```

| Function      | Description        | Example                   |
| ------------- | ------------------ | ------------------------- |
| `keys(obj)`   | Array of key names | `keys({a: 1})` → `["a"]`  |
| `values(obj)` | Array of values    | `values({a: 1})` → `[1]`  |
| `len(obj)`    | Number of keys     | `len({a: 1, b: 2})` → `2` |

### Object Helper Functions

Forge provides helper functions that make object manipulation safer and more expressive:

**`has_key(object, key)`** — returns `true` if the key exists:

```forge
let user = { name: "Alice", age: 30 }
say has_key(user, "name")
say has_key(user, "email")
```

Output:

```
true
false
```

**`get(object, key, default)`** — safe access with fallback. Supports dot-paths for nested access:

```forge
let config = { a: { b: { c: "found" } } }
say get(config, "a.b.c", "fallback")
say get(config, "a.b.missing", "fallback")
```

Output:

```
found
fallback
```

**`pick(object, [keys])`** — extract specific fields into a new object:

```forge
let user = { name: "Alice", age: 30, role: "engineer" }
let subset = pick(user, ["name", "role"])
say subset
```

Output:

```
{name: Alice, role: engineer}
```

**`omit(object, [keys])`** — remove specific fields, return a new object:

```forge
let user = { name: "Alice", age: 30, role: "engineer" }
let without_age = omit(user, ["age"])
say without_age
```

Output:

```
{name: Alice, role: engineer}
```

**`merge(obj1, obj2, ...)`** — combine objects. Later objects win on key conflicts:

```forge
let defaults = { theme: "dark", fontSize: 14 }
let overrides = { fontSize: 18 }
let merged = merge(defaults, overrides)
say merged
```

Output:

```
{theme: dark, fontSize: 18}
```

**`entries(object)`** — convert to an array of `[key, value]` pairs:

```forge
let scores = { alice: 95, bob: 87 }
say entries(scores)
```

Output:

```
[[alice, 95], [bob, 87]]
```

**`from_entries(pairs)`** — convert pairs back to an object:

```forge
let pairs = [["x", 1], ["y", 2], ["z", 3]]
let obj = from_entries(pairs)
say obj
```

Output:

```
{x: 1, y: 2, z: 3}
```

**`contains(object, key)`** — check if a key exists. Also works on strings (substring) and arrays (membership):

```forge
let data = { a: 1, b: 2 }
say contains(data, "a")
say contains("hello", "ell")
say contains([1, 2, 3], 2)
```

Output:

```
true
true
true
```

### Object Iteration

Iterate over an object to access both keys and values:

```forge
let scores = { alice: 95, bob: 87, charlie: 92 }

for name, score in scores {
    say "{name} scored {score}"
}
```

Output:

```
alice scored 95
bob scored 87
charlie scored 92
```

### Arrays of Objects

One of the most common data patterns is an array of objects — essentially a table of records:

```forge
let employees = [
    { name: "Alice", department: "Engineering", salary: 95000 },
    { name: "Bob", department: "Design", salary: 82000 },
    { name: "Charlie", department: "Engineering", salary: 105000 },
    { name: "Diana", department: "Marketing", salary: 78000 },
    { name: "Eve", department: "Engineering", salary: 98000 }
]

let engineers = filter(employees, fn(e) {
    return e.department == "Engineering"
})

say "Engineers: {len(engineers)}"

for e in engineers {
    say "  {e.name}: ${e.salary}"
}
```

Output:

```
Engineers: 3
  Alice: $95000
  Charlie: $105000
  Eve: $98000
```

### Building Data Pipelines with Collections

Combining arrays, objects, and functional operations creates powerful data processing pipelines:

```forge
let orders = [
    { product: "Widget", quantity: 5, price: 9.99 },
    { product: "Gadget", quantity: 2, price: 24.99 },
    { product: "Doohickey", quantity: 10, price: 4.99 },
    { product: "Thingamajig", quantity: 1, price: 49.99 },
    { product: "Widget", quantity: 3, price: 9.99 }
]

let totals = map(orders, fn(order) {
    return {
        product: order.product,
        total: order.quantity * order.price
    }
})

let grand_total = reduce(totals, 0.0, fn(acc, item) {
    return acc + item.total
})

say "Order Summary:"
for item in totals {
    say "  {item.product}: ${item.total}"
}
say "Grand Total: ${grand_total}"
```

Output:

```
Order Summary:
  Widget: $49.95
  Gadget: $49.98
  Doohickey: $49.9
  Thingamajig: $49.99
  Widget: $29.97
Grand Total: $229.79
```

Here is another pipeline that filters, transforms, and summarizes:

```forge
let students = [
    { name: "Alice", grade: 92 },
    { name: "Bob", grade: 78 },
    { name: "Charlie", grade: 95 },
    { name: "Diana", grade: 88 },
    { name: "Eve", grade: 71 }
]

let honor_roll = filter(students, fn(s) { return s.grade >= 90 })
let honor_names = map(honor_roll, fn(s) { return s.name })
say "Honor Roll: {honor_names}"

let grades = map(students, fn(s) { return s.grade })
let avg = reduce(grades, 0, fn(acc, g) { return acc + g }) / len(students)
say "Class Average: {avg}"
```

Output:

```
Honor Roll: [Alice, Charlie]
Class Average: 84
```

### String Operations as Collection Tools

Strings behave like collections of characters in some contexts. Several built-in functions bridge between strings and arrays:

```forge
let sentence = "hello world from forge"
let words = split(sentence, " ")
say words

let result = join(words, "-")
say result

say replace(sentence, "forge", "Forge")
say starts_with(sentence, "hello")
say ends_with(sentence, "forge")
```

Output:

```
[hello, world, from, forge]
hello-world-from-forge
hello world from Forge
true
true
```

### The lines() Function

The `lines()` function splits a string on newline characters and returns an array of lines:

```forge
let text = "line1\nline2\nline3"
let lines = lines(text)
say lines
```

Output:

```
[line1, line2, line3]
```

Useful for processing multi-line input, log files, or any text with line breaks.

### The find() Function

The `find(array, fn)` function returns the first element that matches the predicate, or `null` if none match:

```forge
let numbers = [3, 7, 2, 9, 4]
let first = find(numbers, fn(x) { return x > 5 })
say first

let none = find(numbers, fn(x) { return x > 100 })
say none
```

Output:

```
7
null
```

It also works as a method:

```forge
let nums = [1, 4, 9, 16, 25]
let first_large = nums.find(fn(x) { return x > 10 })
say first_large
```

Output:

```
16
```

### Try It Yourself

1. **Word Counter.** Given the string `"the quick brown fox jumps over the lazy dog"`, split it into words, then use `reduce` to count how many words have more than 3 letters. (Expected: 6.)

2. **Student Report.** Create an array of 5 student objects, each with `name` and `score` fields. Use `filter` to find students with scores above 85, `map` to create a greeting for each ("Congratulations, [name]!"), and print the results.

3. **Object Builder.** Write a program that starts with an empty array, uses `push` to add 5 objects (each with `id` and `value` fields), then uses `reduce` to compute the sum of all `value` fields. Print the array and the total.

---

## Chapter 8: Error Handling

Most programs encounter errors: files that don't exist, network connections that fail, invalid user input. Forge takes the position that errors should be _values_, not invisible exceptions that surprise you. This chapter covers Forge's comprehensive error-handling system, from Result types to the `safe` block.

### Philosophy: Errors as Values

In many languages, errors are _exceptions_ — they fly up the call stack invisibly until something catches them, or they crash the program. This model has two problems: you can't tell which functions might throw just by reading the code, and forgetting to catch an exception means a silent crash.

Forge follows the errors-as-values philosophy pioneered by Rust and Go. A function that can fail returns a `Result` — a wrapper that is either `Ok(value)` on success or `Err("message")` on failure. You handle the result explicitly, and the compiler helps you remember.

Think of it like ordering food. In the exception model, you order and hope for the best — if the kitchen is on fire, someone runs out screaming. In the errors-as-values model, the waiter brings you a tray with either your food or a note explaining what went wrong. Either way, you know what happened.

### Result Types: Ok and Err

Create successful and failed results:

```forge
let success = Ok(42)
let failure = Err("something went wrong")

say success
say failure
```

Output:

```
Ok(42)
Err(something went wrong)
```

### Creating and Inspecting Results

Functions that can fail conventionally return `Ok` or `Err`:

```forge
fn safe_divide(a, b) {
    if b == 0 {
        return Err("division by zero")
    }
    return Ok(a / b)
}

let result1 = safe_divide(10, 2)
let result2 = safe_divide(10, 0)

say result1
say result2
say is_ok(result1)
say is_err(result2)
```

Output:

```
Ok(5)
Err(division by zero)
true
true
```

Use `is_ok()` and `is_err()` to check the state of a Result before extracting its value:

```forge
let result = safe_divide(42, 6)
if is_ok(result) {
    say "Value: {unwrap(result)}"
}
```

Output:

```
Value: 7
```

### Pattern Matching on Results

The most common way to handle Results is with `match`:

```forge
fn parse_positive(input) {
    let n = int(input)
    if n < 0 {
        return Err("expected a positive integer")
    }
    return Ok(n)
}

match parse_positive("42") {
    Ok(value) => say "Got: {value}"
    Err(msg) => say "Error: {msg}"
}

match parse_positive("-5") {
    Ok(value) => say "Got: {value}"
    Err(msg) => say "Error: {msg}"
}
```

Output:

```
Got: 42
Error: expected a positive integer
```

Pattern matching forces you to handle both cases. You cannot accidentally ignore an error because the `match` expression requires arms for both `Ok` and `Err`.

### The ? Operator (Error Propagation)

The `?` operator is Forge's most ergonomic error-handling tool. When applied to a Result, it:

- Extracts the value if the Result is `Ok`
- Immediately returns the `Err` from the enclosing function if the Result is `Err`

```forge
fn parse_positive_int(input) {
    let n = int(input)
    if n < 0 {
        return Err("expected a positive integer")
    }
    return Ok(n)
}

fn double_positive(input) {
    let n = parse_positive_int(input)?
    return Ok(n * 2)
}

let good = double_positive("21")
let bad = double_positive("-5")

say good
say bad
```

Output:

```
Ok(42)
Err(expected a positive integer)
```

Without `?`, you would have to manually check every result:

```forge
fn double_positive_verbose(input) {
    let result = parse_positive_int(input)
    if is_err(result) {
        return result
    }
    let n = unwrap(result)
    return Ok(n * 2)
}
```

The `?` operator collapses those three lines into one. This is especially valuable when you have multiple operations that can fail:

```forge
fn process_data(a_str, b_str) {
    let a = parse_positive_int(a_str)?
    let b = parse_positive_int(b_str)?
    if b == 0 {
        return Err("second value cannot be zero")
    }
    return Ok(a / b)
}

say process_data("10", "3")
say process_data("10", "-1")
say process_data("10", "0")
```

Output:

```
Ok(3)
Err(expected a positive integer)
Err(second value cannot be zero)
```

> **Tip:** The `?` operator only works inside functions that return a `Result`. If you use it in top-level code, the error will propagate as a runtime error.

### try/catch Blocks

For situations where you want to handle errors from code that might crash (like division by zero), use `try/catch`:

```forge
try {
    let x = 1 / 0
} catch err {
    say "Caught: {err}"
}
```

Output:

```
Caught: division by zero
```

The `try` block runs the code inside it. If a runtime error occurs, execution jumps to the `catch` block, which receives the error message as a string:

```forge
try {
    let data = [1, 2, 3]
    say data[10]
} catch err {
    say "Error accessing array: {err}"
}
say "Program continues normally"
```

Output:

```
Error accessing array: index out of bounds
Program continues normally
```

### safe Blocks

The `safe` block is Forge's simplest error suppression mechanism. Code inside a `safe` block runs, but any error is silently caught and the program continues:

```forge
safe {
    let x = 1 / 0
}
say "I survived a division by zero!"
```

Output:

```
I survived a division by zero!
```

`safe` is useful for operations where failure is acceptable — fire-and-forget logging, optional cleanup, best-effort operations. Use it sparingly, as silencing errors can mask bugs.

> **Tip:** Prefer `try/catch` over `safe` when you want to know _what_ went wrong. Use `safe` only when you genuinely don't care whether the code succeeds.

### The must Keyword

The `must` keyword is the opposite of `safe` — it asserts that a Result is `Ok` and crashes the program if it is not:

```forge
let value = must Ok(42)
say "Value: {value}"
```

Output:

```
Value: 42
```

If the Result is an `Err`, `must` terminates the program with a clear error message:

```forge
let value = must Err("catastrophic failure")
```

This would crash with a message about the error. Use `must` when an error is truly unrecoverable — for example, failing to read a configuration file that your program cannot function without:

```forge
fn load_config(path) {
    if !fs.exists(path) {
        return Err("config file not found: {path}")
    }
    let content = fs.read(path)
    return Ok(content)
}

let config = must load_config("app.toml")
```

### The check Statement

The `check` statement performs declarative validation:

```forge
let name = "Alice"
check name
```

If `name` were empty, `check` would raise an error. The `check` statement validates that a value is truthy — it is a concise way to assert preconditions:

```forge
fn create_user(name, email) {
    check name
    check email
    return { name: name, email: email }
}

let user = create_user("Alice", "alice@example.com")
say user
```

Output:

```
{name: Alice, email: alice@example.com}
```

### unwrap and unwrap_or

The `unwrap()` function extracts the value from an `Ok` result. If the result is `Err`, it crashes:

```forge
let result = Ok(42)
say unwrap(result)
```

Output:

```
42
```

For a safer alternative, use `unwrap_or()` to provide a default value:

```forge
let good = Ok(42)
let bad = Err("failed")

say unwrap_or(good, 0)
say unwrap_or(bad, 0)
```

Output:

```
42
0
```

`unwrap_or` never crashes. If the Result is `Err`, it returns the default value instead.

### Option Types

Forge also has `Option` types for values that may or may not exist:

```forge
let x = Some(42)
let y = None

say is_some(x)
say is_none(y)

match x {
    Some(val) => say "Got: {val}"
    None => say "Nothing"
}
```

Output:

```
true
true
Got: 42
```

Options are used when a value might be absent without that being an error. For example, looking up a key in a map might return `Some(value)` or `None`.

### Error Messages and Suggestions

Forge strives to produce helpful error messages. When you make a common mistake, the runtime often suggests a correction:

```forge
let x = 10
x = 20
```

Error:

```
cannot reassign immutable variable 'x'
  hint: declare with 'let mut x' to make it mutable
```

Division by zero:

```
division by zero
  hint: check that the divisor is not zero before dividing
```

These contextual hints are part of Forge's design philosophy: errors should teach, not just complain.

### Best Practices

1. **Use Result types for functions that can fail.** Return `Ok` on success, `Err` on failure. This makes the failure mode visible in the function signature.

2. **Use `?` to propagate errors.** Don't manually check every result. The `?` operator keeps your code clean and ensures errors bubble up naturally.

3. **Use `match` for handling Results.** It forces you to consider both the success and failure cases.

4. **Reserve `must` for truly unrecoverable errors.** Configuration loading, database connection — things the program cannot proceed without.

5. **Use `safe` sparingly.** Silencing errors is occasionally necessary, but most errors deserve to be handled explicitly.

6. **Use `try/catch` for code that might crash unexpectedly.** Division by zero, array index out of bounds, type conversion failures.

7. **Prefer `unwrap_or` over `unwrap`.** It provides a graceful fallback instead of crashing.

```forge
fn read_config(path) {
    if !fs.exists(path) {
        return Err("config file not found")
    }
    return Ok(fs.read(path))
}

fn start_server() {
    let config = read_config("server.toml")?
    say "Starting with config: {config}"
    return Ok(true)
}

match start_server() {
    Ok(_) => say "Server started"
    Err(msg) => say "Failed to start: {msg}"
}
```

This pattern — functions returning Results, `?` propagating errors, `match` handling them at the top level — is the idiomatic way to handle errors in Forge.

### Try It Yourself

1. **Safe Division Chain.** Write a function `chain_divide(a, b, c)` that divides `a` by `b`, then divides the result by `c`. Both divisions should be done with a `safe_divide` function that returns `Err` on division by zero. Use the `?` operator to propagate errors. Test with `chain_divide(100, 5, 2)` (expected: `Ok(10)`), `chain_divide(100, 0, 2)` (expected: `Err`), and `chain_divide(100, 5, 0)` (expected: `Err`).

2. **Graceful Defaults.** Write a program that tries to parse three strings as integers using a function that returns `Result`. Use `unwrap_or` to provide a default of `0` for any string that fails to parse. Compute and print the sum. Test with `["42", "not_a_number", "8"]` (expected sum: 50).

3. **Error Reporter.** Write a function `validate_user(name, age_str)` that returns `Err` if the name is empty, `Err` if the age string cannot be parsed as an integer, and `Err` if the age is negative. On success, return `Ok({ name: name, age: age })`. Test with valid input, empty name, invalid age string, and negative age. Use `match` to print a specific message for each case.
