# Comparison and Logical Expressions

Comparison operators produce boolean values by comparing two operands. Logical operators combine boolean expressions using short-circuit evaluation.

## Comparison Operators

| Operator | Meaning               | Example  |
| -------- | --------------------- | -------- |
| `==`     | Equal                 | `x == y` |
| `!=`     | Not equal             | `x != y` |
| `<`      | Less than             | `x < y`  |
| `>`      | Greater than          | `x > y`  |
| `<=`     | Less than or equal    | `x <= y` |
| `>=`     | Greater than or equal | `x >= y` |

### Equality

The `==` operator tests structural equality. Two values are equal if they have the same type and the same content.

```forge
say 1 == 1              // true
say "abc" == "abc"      // true
say [1, 2] == [1, 2]   // true
say null == null        // true
say 1 == 1.0            // true (numeric promotion)
say 1 == "1"            // false (different types)
```

### Numeric Comparison

Integers and floats can be compared directly. When comparing an `int` with a `float`, the integer is promoted to float.

```forge
say 3 < 5           // true
say 3.14 > 2.71     // true
say 10 >= 10        // true
say 5 <= 3          // false
say 1 < 2.5         // true (int promoted to float)
```

### String Comparison

Strings are compared lexicographically (by Unicode code points).

```forge
say "apple" < "banana"      // true
say "abc" == "abc"          // true
say "a" < "b"               // true
```

### Null Comparison

Only `null` is equal to `null`. Comparing `null` with any other value using `==` yields `false`.

```forge
say null == null        // true
say null == 0           // false
say null == ""          // false
say null == false       // false
```

## Logical Operators

Forge supports two syntactic forms for each logical operator. Both forms are equivalent.

| Classic | Natural | Meaning     |
| ------- | ------- | ----------- |
| `&&`    | `and`   | Logical AND |
| `\|\|`  | `or`    | Logical OR  |
| `!`     | `not`   | Logical NOT |

### Logical AND

Returns `true` if both operands are truthy. Uses short-circuit evaluation: if the left operand is falsy, the right operand is not evaluated.

```forge
say true and true       // true
say true and false      // false
say false and true      // false (right side not evaluated)
```

### Logical OR

Returns `true` if either operand is truthy. Uses short-circuit evaluation: if the left operand is truthy, the right operand is not evaluated.

```forge
say false or true       // true
say true or false       // true (right side not evaluated)
say false or false      // false
```

### Logical NOT

Returns the boolean negation of the operand. Applies truthiness rules.

```forge
say not true        // false
say not false       // true
say !null           // true
say not 0           // false (0 is truthy in Forge)
```

## Short-Circuit Evaluation

Short-circuit evaluation means the right operand of `and`/`&&` or `or`/`||` is only evaluated when necessary.

```forge
// The function is never called because the left side is false
false and expensive_computation()

// The function is never called because the left side is true
true or expensive_computation()
```

This is significant when the right operand has side effects:

```forge
let x = null
// Safe: the right side is not evaluated when x is null
x != null and x.name == "Alice"
```

## Truthiness Rules

Forge uses the following truthiness rules when a value appears in a boolean context (such as an `if` condition or a logical operator):

| Value           | Truthiness |
| --------------- | ---------- |
| `false`         | falsy      |
| `null`          | falsy      |
| Everything else | truthy     |

Notably, the following values are **truthy** (unlike some other languages):

- `0` (zero)
- `""` (empty string)
- `[]` (empty array)
- `{}` (empty object)

```forge
if 0 {
    say "zero is truthy"    // this executes
}

if "" {
    say "empty string is truthy"    // this executes
}

if null {
    say "unreachable"
} otherwise {
    say "null is falsy"     // this executes
}
```

## Operator Precedence

From highest to lowest precedence:

1. `not` / `!` (unary)
2. `<`, `>`, `<=`, `>=`
3. `==`, `!=`
4. `and` / `&&`
5. `or` / `||`

```forge
say true or false and false     // true (and binds tighter than or)
say not false and true          // true (not binds tightest)
```

See the [Operator Precedence](../appendix/precedence.md) appendix for the full table.
