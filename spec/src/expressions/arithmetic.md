# Arithmetic Expressions

Arithmetic expressions perform numeric computations and string concatenation.

## Operators

| Operator | Name           | Operand Types                  | Result Type       |
| -------- | -------------- | ------------------------------ | ----------------- |
| `+`      | Addition       | `int + int`                    | `int`             |
| `+`      | Addition       | `float + float`                | `float`           |
| `+`      | Addition       | `int + float` or `float + int` | `float`           |
| `+`      | Concatenation  | `string + string`              | `string`          |
| `-`      | Subtraction    | `int - int`                    | `int`             |
| `-`      | Subtraction    | `float - float`                | `float`           |
| `-`      | Subtraction    | `int - float` or `float - int` | `float`           |
| `*`      | Multiplication | `int * int`                    | `int`             |
| `*`      | Multiplication | `float * float`                | `float`           |
| `*`      | Multiplication | `int * float` or `float - int` | `float`           |
| `/`      | Division       | `int / int`                    | `int` (truncated) |
| `/`      | Division       | `float / float`                | `float`           |
| `/`      | Division       | `int / float` or `float / int` | `float`           |
| `%`      | Modulo         | `int % int`                    | `int`             |
| `%`      | Modulo         | `float % float`                | `float`           |

## Integer Arithmetic

Integer arithmetic uses 64-bit signed integers (`i64`). Operations that overflow follow Rust's default behavior (panic in debug, wrap in release).

```forge
let a = 10 + 3      // 13
let b = 10 - 3      // 7
let c = 10 * 3      // 30
let d = 10 / 3      // 3 (truncated toward zero)
let e = 10 % 3      // 1
```

### Integer Division

Integer division truncates toward zero. The result is always an integer when both operands are integers.

```forge
say 7 / 2       // 3
say -7 / 2      // -3
say 7 / -2      // -3
```

## Float Arithmetic

Float arithmetic uses 64-bit IEEE 754 double-precision floating-point numbers (`f64`).

```forge
let a = 3.14 + 2.0     // 5.14
let b = 10.0 / 3.0     // 3.3333333333333335
let c = 2.5 % 1.0      // 0.5
```

## Mixed Arithmetic

When one operand is `int` and the other is `float`, the integer is promoted to a float before the operation. The result is always `float`.

```forge
say 5 + 2.0      // 7.0
say 10 / 3.0     // 3.3333333333333335
say 3 * 1.5      // 4.5
```

## String Concatenation

The `+` operator concatenates two strings. If one operand is a string and the other is not, the non-string operand is converted to its string representation.

```forge
say "hello" + " " + "world"    // "hello world"
say "count: " + str(42)        // "count: 42"
```

For embedding expressions in strings, prefer [string interpolation](./string-interpolation.md):

```forge
say "count: {42}"              // "count: 42"
```

## Unary Negation

The `-` prefix operator negates a numeric value.

```forge
let x = 5
say -x          // -5
say -3.14       // -3.14
```

## Operator Precedence

Arithmetic operators follow standard mathematical precedence:

1. Unary `-` (highest)
2. `*`, `/`, `%`
3. `+`, `-` (lowest among arithmetic)

Parentheses override precedence:

```forge
say 2 + 3 * 4       // 14
say (2 + 3) * 4     // 20
```

See the [Operator Precedence](../appendix/precedence.md) appendix for the full precedence table including all operator categories.

## Division by Zero

Dividing by zero produces a runtime error:

```forge
say 10 / 0      // runtime error: division by zero
```

Float division by zero follows IEEE 754 rules and may produce infinity or NaN.
