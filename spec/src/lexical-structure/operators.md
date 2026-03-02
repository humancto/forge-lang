# Operators and Punctuation

This section defines all operator and punctuation tokens in Forge.

## Arithmetic Operators

| Token | Name    | Example | Description                    |
| ----- | ------- | ------- | ------------------------------ |
| `+`   | Plus    | `a + b` | Addition; string concatenation |
| `-`   | Minus   | `a - b` | Subtraction; unary negation    |
| `*`   | Star    | `a * b` | Multiplication                 |
| `/`   | Slash   | `a / b` | Division                       |
| `%`   | Percent | `a % b` | Modulo (remainder)             |

When both operands of `/` are integers, the result is an integer (truncating division). When either operand is a float, the result is a float.

The `+` operator is overloaded for string concatenation when both operands are strings.

## Comparison Operators

| Token | Name                  | Example  | Description             |
| ----- | --------------------- | -------- | ----------------------- |
| `==`  | Equal                 | `a == b` | Equality test           |
| `!=`  | Not equal             | `a != b` | Inequality test         |
| `<`   | Less than             | `a < b`  | Less-than comparison    |
| `>`   | Greater than          | `a > b`  | Greater-than comparison |
| `<=`  | Less than or equal    | `a <= b` | Less-than-or-equal      |
| `>=`  | Greater than or equal | `a >= b` | Greater-than-or-equal   |

All comparison operators return a `Bool` value. Strings are compared lexicographically.

## Logical Operators

| Token  | Name        | Example    | Description               |
| ------ | ----------- | ---------- | ------------------------- |
| `&&`   | Logical AND | `a && b`   | Short-circuit conjunction |
| `\|\|` | Logical OR  | `a \|\| b` | Short-circuit disjunction |
| `!`    | Logical NOT | `!a`       | Unary boolean negation    |

The keywords `and` and `or` are **not** reserved keywords in Forge. Logical operations use the symbolic `&&` and `||` forms exclusively. The `not` keyword is also not reserved; use the `!` prefix operator.

Both `&&` and `||` use short-circuit evaluation: the right operand is not evaluated if the result can be determined from the left operand alone.

## Assignment Operators

| Token | Name            | Example  | Equivalent  |
| ----- | --------------- | -------- | ----------- |
| `=`   | Assignment      | `x = 5`  | —           |
| `+=`  | Add-assign      | `x += 5` | `x = x + 5` |
| `-=`  | Subtract-assign | `x -= 3` | `x = x - 3` |
| `*=`  | Multiply-assign | `x *= 2` | `x = x * 2` |
| `/=`  | Divide-assign   | `x /= 4` | `x = x / 4` |

Assignment and compound assignment operators require the left-hand side to be a mutable variable (declared with `mut`). Compound assignment is syntactic sugar for the expanded form.

## Member Access and Navigation

| Token | Name  | Example     | Description                       |
| ----- | ----- | ----------- | --------------------------------- |
| `.`   | Dot   | `obj.field` | Field access, method call         |
| `..`  | Range | `1..10`     | Range constructor (exclusive end) |

The dot operator accesses fields on objects and struct instances, and invokes methods. It binds very tightly (highest precedence among binary operators).

The range operator `..` creates a range value from a start (inclusive) to an end (exclusive). It is used primarily with `for` loops and the `range()` built-in.

## Pipe Operators

| Token | Name       | Example   | Description                                          |
| ----- | ---------- | --------- | ---------------------------------------------------- |
| `\|>` | Pipe       | `x \|> f` | Pipe-forward: passes left as first argument to right |
| `>>`  | Pipe right | `x >> f`  | Alternate pipe operator                              |

The pipe operator passes the value on the left as the first argument to the function on the right:

```forge
let result = [3, 1, 4, 1, 5]
    |> sort
    |> reverse
```

## Spread Operator

| Token | Name   | Example       | Description                   |
| ----- | ------ | ------------- | ----------------------------- |
| `...` | Spread | `[...arr, 4]` | Spreads array/object elements |

The spread operator expands an array or object into individual elements within an array literal or object literal:

```forge
let a = [1, 2, 3]
let b = [...a, 4, 5]       // [1, 2, 3, 4, 5]

let base = { x: 1, y: 2 }
let ext = { ...base, z: 3 } // { x: 1, y: 2, z: 3 }
```

## Arrow Operators

| Token | Name      | Example          | Description                     |
| ----- | --------- | ---------------- | ------------------------------- |
| `->`  | Arrow     | `< 13 -> "kid"`  | Arm separator in `when`/`match` |
| `=>`  | Fat arrow | `Ok(v) => say v` | Arm separator in `match`        |

The thin arrow `->` is used in `when` guard arms. The fat arrow `=>` is used in `match` pattern arms. Both separate a pattern/condition from its corresponding body.

## Special Operators

| Token | Name      | Example                   | Description                       |
| ----- | --------- | ------------------------- | --------------------------------- |
| `?`   | Question  | `expr?`                   | Error propagation (Result/Option) |
| `@`   | At        | `@test`                   | Decorator prefix                  |
| `&`   | Ampersand | (reserved)                | Reserved for future use           |
| `\|`  | Bar       | `Circle(r) \| Rect(w, h)` | ADT variant separator             |

The `?` postfix operator propagates errors: if the expression evaluates to `Err(e)`, the enclosing function returns `Err(e)` immediately. If the expression is `Ok(v)`, the `?` unwraps it to `v`.

## Delimiters

| Token | Name          | Purpose                              |
| ----- | ------------- | ------------------------------------ |
| `(`   | Left paren    | Function call, grouping              |
| `)`   | Right paren   | Close function call, grouping        |
| `{`   | Left brace    | Block, object literal, interpolation |
| `}`   | Right brace   | Close block, object, interpolation   |
| `[`   | Left bracket  | Array literal, index access          |
| `]`   | Right bracket | Close array, index access            |
| `,`   | Comma         | Separator in lists                   |
| `:`   | Colon         | Key-value separator, type annotation |
| `;`   | Semicolon     | Optional statement terminator        |

## Operator Precedence

Operators are listed from highest to lowest precedence:

| Precedence  | Operators                   | Associativity |
| ----------- | --------------------------- | ------------- |
| 1 (highest) | `.` (member access)         | Left-to-right |
| 2           | `()` (call), `[]` (index)   | Left-to-right |
| 3           | `!`, `-` (unary)            | Right-to-left |
| 4           | `*`, `/`, `%`               | Left-to-right |
| 5           | `+`, `-`                    | Left-to-right |
| 6           | `..`                        | Left-to-right |
| 7           | `<`, `>`, `<=`, `>=`        | Left-to-right |
| 8           | `==`, `!=`                  | Left-to-right |
| 9           | `&&`                        | Left-to-right |
| 10          | `\|\|`                      | Left-to-right |
| 11          | `\|>`                       | Left-to-right |
| 12          | `?`                         | Postfix       |
| 13 (lowest) | `=`, `+=`, `-=`, `*=`, `/=` | Right-to-left |

Parentheses may be used to override the default precedence.
