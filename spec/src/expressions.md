# Expressions

An **expression** is a syntactic construct that evaluates to a value. Every expression in Forge has a type and produces a result when evaluated.

Forge distinguishes expressions from [statements](./statements.md): expressions produce values, statements produce effects. An expression can appear anywhere a value is expected -- as the right-hand side of a variable binding, as a function argument, or as the body of a `when` arm.

## Expression Categories

### Literal Expressions

Literal expressions produce values directly from source text.

| Literal | Example                         | Value Type |
| ------- | ------------------------------- | ---------- |
| Integer | `42`                            | `int`      |
| Float   | `3.14`                          | `float`    |
| String  | `"hello"`                       | `string`   |
| Boolean | `true`, `false`                 | `bool`     |
| Null    | `null`                          | `null`     |
| Array   | `[1, 2, 3]`                     | `array`    |
| Object  | `{ name: "Forge", version: 1 }` | `object`   |

String literals support [interpolation](./expressions/string-interpolation.md) with embedded expressions.

### Identifier Expressions

An identifier evaluates to the value bound to that name in the current scope.

```forge
let x = 10
say x       // evaluates to 10
```

If the identifier is not in scope, evaluation produces a runtime error.

### Arithmetic Expressions

Binary operations on numeric values: `+`, `-`, `*`, `/`, `%`. The `+` operator also performs string concatenation.

See [Arithmetic](./expressions/arithmetic.md).

### Comparison and Logical Expressions

Comparison operators (`==`, `!=`, `<`, `>`, `<=`, `>=`) produce boolean values. Logical operators (`and`/`&&`, `or`/`||`, `not`/`!`) combine boolean expressions with short-circuit evaluation.

See [Comparison and Logical](./expressions/comparison.md).

### Field Access Expressions

Dot notation accesses fields on objects and struct instances. Embedded fields are resolved through delegation.

```forge
let user = { name: "Alice", age: 30 }
say user.name   // "Alice"
```

See [Field Access](./expressions/field-access.md).

### Index Expressions

Bracket notation accesses elements by position in arrays or by key in objects.

```forge
let items = [10, 20, 30]
say items[0]    // 10

let obj = { x: 1 }
say obj["x"]    // 1
```

### Method Call Expressions

Method calls use dot notation followed by a function call. Forge resolves methods through a multi-step dispatch chain: object fields, type method tables, embedded field delegation, then known built-in methods.

```forge
let names = ["Charlie", "Alice", "Bob"]
say names.sort()    // ["Alice", "Bob", "Charlie"]
```

See [Method Calls](./expressions/method-calls.md).

### Function Call Expressions

A function call evaluates a callable expression and applies it to a list of argument expressions.

```forge
fn square(n) { n * n }
say square(5)   // 25
```

### Closures and Lambdas

Anonymous functions that capture their enclosing environment.

```forge
let double = fn(x) { x * 2 }
say double(5)   // 10
```

See [Closures and Lambdas](./expressions/closures.md).

### When Guard Expressions

Pattern-matching on a scrutinee value using comparison operators.

```forge
let label = when age {
    < 13 -> "child",
    < 18 -> "teen",
    else -> "adult"
}
```

See [When Guards](./expressions/when-guards.md).

### Match Expressions

Structural pattern matching with destructuring of algebraic data types.

```forge
match shape {
    Circle(r) => say "radius: {r}",
    Rect(w, h) => say "area: {w * h}",
    _ => say "unknown"
}
```

See [Match Expressions](./expressions/match.md).

### String Interpolation Expressions

Double-quoted strings with embedded expressions in `{...}` delimiters.

```forge
let name = "world"
say "hello, {name}!"   // "hello, world!"
```

See [String Interpolation](./expressions/string-interpolation.md).

### Pipeline Expressions

The pipe operator `|>` threads a value through a chain of function calls.

```forge
[1, 2, 3, 4, 5]
    |> filter(fn(x) { x > 2 })
    |> map(fn(x) { x * 10 })
```

### Unary Expressions

Unary operators: `-` (numeric negation) and `not`/`!` (logical negation).

```forge
let x = -5
let flag = not true
```

### Try Expressions

The `?` operator propagates errors from `Result` values. If the value is `Err`, evaluation returns early from the enclosing function.

```forge
let data = fs.read("config.json")?
```

### Must Expressions

The `must` keyword unwraps a `Result` or crashes with a descriptive error message.

```forge
let data = must fs.read("config.json")
```

### Struct Initialization Expressions

Creates an instance of a named struct type.

```forge
thing Point { x: int, y: int }
let p = Point { x: 10, y: 20 }
```

### Spread Expressions

The `...` operator expands an array or object in a literal context.

```forge
let base = [1, 2, 3]
let extended = [...base, 4, 5]
```

## Expression Evaluation Order

Forge evaluates expressions left to right. In a binary expression `a + b`, `a` is evaluated before `b`. In a function call `f(x, y)`, `f` is evaluated first, then `x`, then `y`.

Logical operators `and`/`&&` and `or`/`||` use short-circuit evaluation: the right operand is not evaluated if the left operand determines the result. See [Comparison and Logical](./expressions/comparison.md) for details.
