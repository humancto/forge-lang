# Match Expressions

A **match expression** performs structural pattern matching on a value. Each arm specifies a pattern; the first arm whose pattern matches the scrutinee has its body evaluated.

## Syntax

```
match expression {
    pattern => body,
    pattern => body,
    ...
}
```

The **scrutinee** is the expression after `match`. Each **arm** consists of a pattern, the `=>` arrow, and a body (one or more statements). Arms are separated by commas.

## Patterns

Forge supports the following pattern forms:

### Wildcard Pattern

The underscore `_` matches any value and binds nothing.

```forge
match x {
    _ => say "matched anything"
}
```

### Literal Pattern

A literal value matches if the scrutinee is equal to that value.

```forge
match color {
    "red" => say "stop",
    "green" => say "go",
    "yellow" => say "caution",
    _ => say "unknown"
}
```

### Binding Pattern

A bare identifier binds the matched value to that name within the arm body.

```forge
match value {
    x => say "got: {x}"
}
```

### Constructor Pattern

A constructor pattern matches an algebraic data type (ADT) variant and destructures its fields. The pattern names the variant and provides sub-patterns for each field.

```forge
type Shape {
    Circle(float)
    Rect(float, float)
    Point
}

let s = Circle(5.0)

match s {
    Circle(r) => say "circle with radius {r}",
    Rect(w, h) => say "rectangle {w} x {h}",
    Point => say "a point",
    _ => say "unknown shape"
}
```

Nested constructor patterns are supported:

```forge
type Expr {
    Num(int)
    Add(Expr, Expr)
}

match expr {
    Add(Num(a), Num(b)) => say "sum: {a + b}",
    Num(n) => say "number: {n}",
    _ => say "complex expression"
}
```

## Evaluation Semantics

1. The scrutinee expression is evaluated exactly once.
2. Arms are tested top to bottom.
3. For each arm, the pattern is matched against the scrutinee:
   - **Wildcard**: Always matches.
   - **Literal**: Matches if the scrutinee equals the literal value.
   - **Binding**: Always matches; binds the scrutinee to the identifier.
   - **Constructor**: Matches if the scrutinee is an ADT value with the same variant name and the correct number of fields, and all sub-patterns recursively match.
4. The first matching arm's body is evaluated. Bindings introduced by the pattern are in scope for the body.
5. If no arm matches, the match expression evaluates to `null`.

## Match as an Expression

`match` produces a value and can be used in expression position:

```forge
let area = match shape {
    Circle(r) => 3.14159 * r * r,
    Rect(w, h) => w * h,
    _ => 0.0
}
```

## Match as a Statement

`match` can appear at the statement level:

```forge
match event {
    Click(x, y) => handle_click(x, y),
    KeyPress(key) => handle_key(key),
    _ => {}
}
```

## Multi-Statement Arm Bodies

Arm bodies can contain multiple statements. The last expression in the block is the value of the arm.

```forge
let result = match data {
    Some(value) => {
        let processed = transform(value)
        validate(processed)
        processed
    },
    None => default_value()
}
```

## ADT Matching

Match is the primary mechanism for working with algebraic data types (see [Algebraic Data Types](../types/adt.md)).

```forge
type Result {
    Ok(any)
    Err(string)
}

fn handle(r) {
    match r {
        Ok(value) => say "success: {value}",
        Err(msg) => say "error: {msg}"
    }
}
```

## Exhaustiveness

Forge does not currently enforce exhaustive matching. If no arm matches the scrutinee, the match expression evaluates to `null`. Use a wildcard `_` arm as the final arm to ensure all cases are handled.

## Differences from When Guards

| Feature        | `match`                        | `when`                      |
| -------------- | ------------------------------ | --------------------------- |
| Arrow syntax   | `=>`                           | `->`                        |
| Matching style | Structural patterns            | Comparison operators        |
| Destructuring  | Yes (ADT variants)             | No                          |
| Use case       | ADT variants, literal dispatch | Numeric ranges, comparisons |

See [When Guards](./when-guards.md) for operator-based branching.
