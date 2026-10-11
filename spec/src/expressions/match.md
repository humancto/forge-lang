# Match Expressions

A **match expression** performs structural pattern matching on a value. Each arm has a pattern and an optional guard; the first arm whose pattern matches the scrutinee (and whose guard holds) is taken.

## Syntax

```
match expression {
    pattern => body,
    pattern if guard => body,
    pattern | pattern => body,
    ...
}
```

The **scrutinee** is the expression after `match`. Each **arm** consists of a pattern, an optional guard (`if` followed by an expression), the `=>` arrow, and a body. The body is either one statement (usually an expression) or a block `{ ... }`. Arms are separated by commas or newlines.

## Patterns

### Wildcard Pattern

The underscore `_` matches any value and binds nothing.

```forge
let x = 42
match x {
    _ => println("matched anything")
}
```

### Literal Pattern

A literal — an integer (optionally negative), a float, a string, `true` or `false` — matches if the scrutinee equals it (`1` matches `1.0`, as with `==`).

```forge
let color = "green"
match color {
    "red" => println("stop"),
    "green" => println("go"),
    _ => println("unknown")
}
```

### Binding Pattern

A bare identifier matches any value and binds it to that name within the guard and the body. A bare name that is a unit variant of an algebraic type in scope (such as `Dot` below) or `None` is instead a test for that variant.

```forge
let value = 7
match value {
    x => println("got: {x}")
}
```

### Constructor Pattern

A constructor pattern matches an algebraic data type (ADT) variant — or `Ok`, `Err`, `Some` — and matches its fields against sub-patterns, which may be any pattern, including nested constructors.

```forge
type Shape = Circle(Float) | Rect(Float, Float) | Dot

let s = Circle(5.0)
match s {
    Circle(r) => println("circle with radius {r}"),
    Rect(w, h) => println("rectangle {w} x {h}"),
    Dot => println("a dot"),
}

let r = Ok(Some(3))
match r {
    Ok(Some(n)) => println("value {n}"),
    Ok(None) => println("empty"),
    Err(e) => println("error {e}"),
}
```

### Or-Pattern

`p1 | p2 | ...` matches when any alternative matches. Every alternative must bind the same names (a parse error otherwise), so the guard and body can use them whichever alternative matched. Or-patterns may also appear inside constructor fields: `Some(1 | 2)`.

```forge
type Shape = Rect(Float, Float) | Square(Float, Float)

let day = "sun"
let kind = match day {
    "sat" | "sun" => "weekend",
    _ => "weekday",
}
println(kind)

let s = Square(2.0, 2.0)
match s {
    Rect(w, h) | Square(w, h) => println("area {w * h}"),
}
```

## Guards

`pattern if condition` takes the arm only when the pattern matches **and** the condition is truthy. The condition is evaluated after the pattern's bindings are made, so it can use them. When the guard is false, matching continues with the next arm.

```forge
fn classify(n) {
    return match n {
        x if x < 0 => "negative",
        0 => "zero",
        x if x % 2 == 0 => "even",
        _ => "odd",
    }
}
println(classify(-3))
println(classify(8))
println(classify(9))
```

## Evaluation Semantics

1. The scrutinee expression is evaluated exactly once.
2. Arms are tried top to bottom.
3. For each arm, the pattern is matched against the scrutinee:
   - **Wildcard**: always matches.
   - **Literal**: matches if the scrutinee equals the literal.
   - **Binding**: matches (unless the name is a unit variant, see above) and binds the scrutinee.
   - **Constructor**: matches if the scrutinee is that variant and every field sub-pattern matches.
   - **Or-pattern**: matches if any alternative matches; the first matching alternative supplies the bindings.
4. If the pattern matches and the arm has a guard, the guard is evaluated with the bindings in scope; a falsy guard moves on to the next arm.
5. The taken arm's body runs with the bindings in scope. Its value is the value of the `match`.
6. If no arm is taken, it is a runtime error: `non-exhaustive match: no arm matched the value` (code E0026). This applies to statement and expression `match` alike.

## Match as an Expression

`match` produces a value and can be used anywhere an expression can: a `let` initializer, a `return`, a function argument, a lambda body, an arm of another `match`.

```forge
type Shape = Circle(Float) | Rect(Float, Float)

let shape = Rect(2.0, 3.0)
let area = match shape {
    Circle(r) => 3.14159 * r * r,
    Rect(w, h) => w * h,
}
println(area)
println(map([1, 2, 3], fn(n) { match n { 1 => "one", _ => "more" } }))
```

The value of an arm is the value of its body: an expression arm gives the expression's value; a block arm gives the value of its last statement (an expression, or a nested `if` / `match` / `when`), or `null` when that is another kind of statement. A `return` inside an arm returns from the enclosing function.

## Match as a Statement

`match` at the start of a statement is a statement; its arms run for their effects.

```forge
let event = "click"
match event {
    "click" => println("clicked"),
    "key" => println("key pressed"),
    _ => {}
}
```

A `match` that ends a function body is the function's result, like any other trailing expression.

## Multi-Statement Arm Bodies

Arm bodies can contain multiple statements. The last expression in the block is the value of the arm.

```forge
let data = Some(20)
let result = match data {
    Some(value) => {
        let doubled = value * 2
        doubled + 1
    },
    None => 0
}
println(result)
```

A block arm body is always a block: to produce an object literal, wrap it in parentheses: `x => ({ value: x })`.

## Exhaustiveness

The type checker reports a match on an ADT, `Option`, `Result` or `Bool` subject that misses a case (T0011, a warning by default). An arm with a guard covers nothing for this check, since its guard may be false; an or-pattern covers each of its alternatives. Add a final `_ => ...` arm when the cases are open. At run time a value that no arm takes raises E0026.

## Differences from When Guards

| Feature        | `match`                         | `when`                      |
| -------------- | ------------------------------- | --------------------------- |
| Arrow syntax   | `=>`                            | `->`                        |
| Matching style | Structural patterns and guards  | Comparison operators        |
| Destructuring  | Yes (ADT variants)              | No                          |
| Use case       | ADT variants, literal dispatch  | Numeric ranges, comparisons |

See [When Guards](./when-guards.md) for operator-based branching.
