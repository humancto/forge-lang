# Syntax Mapping

Complete mapping between classic and natural forms. Both compile to identical AST nodes.

## Variables

| Classic            | Natural                  | Description       |
| ------------------ | ------------------------ | ----------------- |
| `let x = 5`        | `set x to 5`             | Immutable binding |
| `let mut x = 0`    | `set mut x to 0`         | Mutable binding   |
| `x = 10`           | `change x to 10`         | Reassignment      |
| `let {a, b} = obj` | `unpack {a, b} from obj` | Destructuring     |

```forge
// Classic
let name = "Alice"
let mut count = 0
count = count + 1

// Natural
set name to "Alice"
set mut count to 0
change count to count + 1
```

## Functions

| Classic                | Natural                | Description           |
| ---------------------- | ---------------------- | --------------------- |
| `fn add(a, b) { }`     | `define add(a, b) { }` | Function definition   |
| `async fn fetch() { }` | `forge fetch() { }`    | Async function        |
| `return value`         | `return value`         | Return (same in both) |

```forge
// Classic
fn greet(name) {
    return "Hello, " + name
}

// Natural
define greet(name) {
    return "Hello, " + name
}
```

## Control Flow

| Classic    | Natural         | Description          |
| ---------- | --------------- | -------------------- |
| `else { }` | `otherwise { }` | Else branch          |
| `else { }` | `nah { }`       | Else branch (casual) |
| `else if`  | `otherwise if`  | Else-if branch       |

```forge
// Classic
if x > 0 {
    say "positive"
} else if x == 0 {
    say "zero"
} else {
    say "negative"
}

// Natural
if x > 0 {
    say "positive"
} otherwise if x == 0 {
    say "zero"
} nah {
    say "negative"
}
```

## Output

| Classic           | Natural          | Description           |
| ----------------- | ---------------- | --------------------- |
| `println("text")` | `say "text"`     | Print with newline    |
| `print("text")`   | `print("text")`  | Print without newline |
| --                | `yell "text"`    | Print uppercased      |
| --                | `whisper "text"` | Print lowercased      |

## Types and Structures

| Classic                   | Natural               | Description           |
| ------------------------- | --------------------- | --------------------- |
| `struct User { }`         | `thing User { }`      | Struct definition     |
| `impl User { }`           | `give User { }`       | Method implementation |
| `interface Printable { }` | `power Printable { }` | Interface definition  |
| `enum Color { }`          | `craft Color { }`     | Enum definition       |

```forge
// Classic
struct User {
    name: string,
    age: int
}

impl User {
    fn greet(self) {
        say "Hi, I'm " + self.name
    }
}

// Natural
thing User {
    name: string,
    age: int
}

give User {
    fn greet(self) {
        say "Hi, I'm " + self.name
    }
}
```

## Async / Concurrency

| Classic            | Natural                | Description            |
| ------------------ | ---------------------- | ---------------------- |
| `async fn x() { }` | `forge x() { }`        | Async function         |
| `await expr`       | `hold expr`            | Await an async value   |
| `yield value`      | `emit value`           | Yield from a generator |
| `fetch("url")`     | `grab resp from "url"` | HTTP fetch             |

```forge
// Classic
async fn get_data() {
    let resp = await fetch("https://api.example.com/data")
    return resp
}

// Natural
forge get_data() {
    let resp = hold grab data from "https://api.example.com/data"
    return resp
}
```

## Pattern Matching

| Classic           | Natural           |
| ----------------- | ----------------- |
| `match value { }` | `match value { }` |
| `when value { }`  | `when value { }`  |

Both `match` and `when` are available; `when` supports guard-style syntax unique to Forge (see [Innovation Keywords](innovation.md)).

## Modules

| Classic             | Natural             | Description                     |
| ------------------- | ------------------- | ------------------------------- |
| `has InterfaceName` | `has InterfaceName` | Interface conformance assertion |

The `has` keyword asserts that a type satisfies an interface at the point of declaration.
