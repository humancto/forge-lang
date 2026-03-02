# Operator Precedence

Operators listed from lowest precedence (evaluated last) to highest precedence (evaluated first). Operators at the same precedence level are evaluated according to their associativity.

## Precedence Table

| Level | Operator          | Description              | Associativity |
| ----- | ----------------- | ------------------------ | ------------- |
| 1     | `\|\|`            | Logical OR               | Left          |
| 2     | `&&`              | Logical AND              | Left          |
| 3     | `==` `!=`         | Equality                 | Left          |
| 4     | `<` `>` `<=` `>=` | Comparison               | Left          |
| 5     | `+` `-`           | Addition, subtraction    | Left          |
| 6     | `*` `/` `%`       | Multiply, divide, modulo | Left          |
| 7     | `!` `-` (unary)   | Logical NOT, negation    | Right (unary) |
| 8     | `?`               | Postfix try (Result)     | Left          |
| 9     | `.` `[]` `()`     | Access, index, call      | Left          |

## Special Operators

These operators do not fit neatly into the arithmetic precedence chain.

### Pipe Operator `|>`

```forge
let result = data |> transform |> validate
```

The pipe operator has lower precedence than function calls but higher than assignment. It passes the left-hand value as the first argument to the right-hand function.

### Pipe Right `>>`

```forge
from users >> keep where active >> sort by name >> take 5
```

Used in query-style pipe chains. Evaluated left to right.

### Spread `...`

```forge
let merged = [...arr1, ...arr2]
let combined = { ...obj1, ...obj2 }
```

Prefix operator used inside array and object literals. Not a general expression operator.

### Range `..`

```forge
let r = 1..10
```

Creates a range value. Used primarily in `for` loops and slice operations.

### Arrow `->`

```forge
match x {
    1 -> "one",
    _ -> "other",
}
```

Used in match arms and when arms to separate pattern from result. Not a general operator.

### Fat Arrow `=>`

```forge
let f = (x) => x * 2
```

Lambda shorthand syntax. Separates parameters from body.

## Compound Assignment

| Operator | Equivalent      |
| -------- | --------------- |
| `+=`     | `x = x + value` |
| `-=`     | `x = x - value` |
| `*=`     | `x = x * value` |
| `/=`     | `x = x / value` |

Compound assignment operators have the same precedence as regular assignment (`=`). They are statement-level constructs, not expressions.

## Type Operators

| Operator | Context             | Description             |
| -------- | ------------------- | ----------------------- |
| `:`      | `let x: Int = 5`    | Type annotation         |
| `?`      | `fn f(x: Int?) { }` | Optional type modifier  |
| `<>`     | `Array<Int>`        | Generic type parameters |

Type operators appear only in type annotation positions and do not participate in expression evaluation.

## Examples

```forge
// Precedence determines evaluation order
let x = 2 + 3 * 4        // 14 (not 20)
let y = !true || false    // false (! binds tighter than ||)
let z = 1 < 2 && 3 > 1   // true (&& binds looser than < and >)

// Postfix try with field access
let name = get_user()?.name  // ? applies to get_user(), then .name

// Pipe with arithmetic
let result = 5 + 3 |> double  // double(8), not 5 + double(3)
```

## Gotchas

- Unary `-` binds tighter than binary operators: `-2 * 3` is `(-2) * 3 = -6`, not `-(2 * 3) = -6` (same result in this case, but matters for method calls).
- The `?` operator binds tighter than `.`, so `expr?.field` works as expected: it tries `expr`, then accesses `.field` on the result.
- There is no ternary `? :` operator. Use `if/else` expressions or `when` guards instead.
- `==` and `!=` compare by value for all types. There is no identity comparison operator.
