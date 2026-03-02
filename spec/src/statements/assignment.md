# Assignment

Assignment statements change the value of an existing mutable binding. The target must have been declared with `mut` (or `let mut` / `set mut`).

## Simple Assignment

### Classic Syntax

```forge
let mut x = 10
x = 20
say x       // 20
```

### Natural Syntax

```forge
set mut x to 10
change x to 20
say x       // 20
```

Both forms evaluate the right-hand expression and store the result in the named variable.

## Mutability Requirement

Only variables declared with `mut` can be reassigned. Attempting to assign to an immutable variable produces a runtime error:

```forge
let x = 10
x = 20          // runtime error: cannot reassign immutable variable 'x'
```

## Compound Assignment

Compound assignment operators combine an arithmetic operation with assignment. The target must be mutable.

| Operator | Equivalent To |
| -------- | ------------- |
| `x += y` | `x = x + y`   |
| `x -= y` | `x = x - y`   |
| `x *= y` | `x = x * y`   |
| `x /= y` | `x = x / y`   |

```forge
let mut count = 0
count += 1          // count is 1
count += 5          // count is 6
count *= 2          // count is 12
count -= 3          // count is 9
count /= 3          // count is 3
```

Compound assignment with `+=` on strings performs concatenation:

```forge
let mut msg = "hello"
msg += " world"
say msg     // "hello world"
```

## Field Assignment

Fields on objects and struct instances can be assigned using dot notation:

```forge
let mut user = { name: "Alice", age: 30 }
user.age = 31
say user.age    // 31
```

Nested field assignment is supported:

```forge
let mut config = { server: { port: 8080 } }
config.server.port = 3000
say config.server.port  // 3000
```

## Index Assignment

Array elements and object keys can be assigned using bracket notation:

```forge
let mut items = [10, 20, 30]
items[1] = 99
say items       // [10, 99, 30]

let mut obj = { a: 1, b: 2 }
obj["a"] = 100
say obj.a       // 100
```

## Assignment Is Not an Expression

In Forge, assignment is a statement, not an expression. Assignment does not produce a value and cannot be used in expression position:

```forge
// This is NOT valid:
// let y = (x = 5)

// Use separate statements:
let mut x = 0
x = 5
let y = x
```

## Evaluation Order

In an assignment `target = expression`, the right-hand expression is evaluated first, then the result is stored in the target location.

For compound assignment `target += expression`, the current value of the target is read, the operation is performed with the right-hand expression, and the result is stored back.
