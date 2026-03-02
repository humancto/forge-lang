# Algebraic Data Types

Algebraic data types (ADTs) define a type as a fixed set of **variants**. Each variant may optionally carry data. ADTs enable exhaustive pattern matching — the compiler/runtime can verify that all variants are handled.

## Definition

An ADT is defined with the `type` keyword, listing variants separated by `|`:

> _ADTDef_ → `type` _Identifier_ `=` _Variant_ ( `|` _Variant_ )\*
>
> _Variant_ → _Identifier_ ( `(` _TypeList_ `)` )?
>
> _TypeList_ → _Type_ ( `,` _Type_ )\*

### Unit Variants

Variants without data fields are called unit variants:

```forge
type Color = Red | Green | Blue
```

Unit variants are used as simple enumeration values:

```forge
set c to Red
say c  // Red
```

### Data Variants

Variants may carry typed data fields:

```forge
type Shape = Circle(Float) | Rect(Float, Float)
```

Data variants are constructed by calling the variant name as a function:

```forge
set circle to Circle(5.0)
set rect to Rect(3.0, 4.0)
say circle  // Circle(5.0)
say rect    // Rect(3.0, 4.0)
```

### Mixed Variants

An ADT may freely mix unit variants and data variants:

```forge
type Result = Ok(String) | Err(String) | Pending
```

## Construction

Unit variants are referenced by name alone:

```forge
let color = Red
```

Data variants are constructed by calling the variant name with the appropriate arguments:

```forge
let shape = Circle(5.0)
let rect = Rect(3.0, 4.0)
```

The number and types of arguments must match the variant definition.

## Pattern Matching

ADT values are destructured using `match` expressions. Each arm matches a variant and optionally binds its data fields to variables.

### Basic Matching

```forge
type Color = Red | Green | Blue

let c = Red

match c {
    Red => say "Red!"
    Green => say "Green!"
    Blue => say "Blue!"
}
```

### Destructuring Data Variants

Data fields are bound to named variables in the match arm:

```forge
type Shape = Circle(Float) | Rect(Float, Float)

define describe_shape(s) {
    match s {
        Circle(r) => {
            say "Circle with radius {r}, area = {3.14159 * r * r}"
        }
        Rect(w, h) => {
            say "Rectangle {w}x{h}, area = {w * h}"
        }
    }
}

describe_shape(Circle(5.0))
// Output: Circle with radius 5.0, area = 78.53975

describe_shape(Rect(3.0, 4.0))
// Output: Rectangle 3.0x4.0, area = 12.0
```

### Match as Expression

`match` can be used as an expression that returns a value:

```forge
let area = match shape {
    Circle(r) => 3.14159 * r * r
    Rect(w, h) => w * h
}
```

### Exhaustiveness

A `match` expression on an ADT should handle all variants. If a variant is missing, the runtime will produce an error when an unhandled variant is encountered.

```forge
type Color = Red | Green | Blue

// This handles all variants
match c {
    Red => say "Red!"
    Green => say "Green!"
    Blue => say "Blue!"
}
```

### Wildcard Pattern

The `_` pattern matches any value, serving as a catch-all:

```forge
match c {
    Red => say "It's red"
    _ => say "It's not red"
}
```

## Built-in ADTs

Forge provides two built-in algebraic types:

- **Option**: `Some(value) | None` — see [Option and Result](./option-result.md)
- **Result**: `Ok(value) | Err(message)` — see [Option and Result](./option-result.md)

These follow the same pattern matching conventions as user-defined ADTs:

```forge
let x = Some(42)

match x {
    Some(val) => say "Got: {val}"
    None => say "Nothing"
}
```

## Scope

Variant constructors (e.g., `Red`, `Circle`, `Some`, `Ok`) are introduced into the scope where the `type` definition appears. For built-in types like `Option` and `Result`, the constructors are globally available.
