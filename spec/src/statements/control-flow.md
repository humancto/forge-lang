# Control Flow

Control flow statements direct the order of execution based on conditions.

## If Statements

The `if` statement executes a block of code when a condition is truthy.

```forge
if temperature > 100 {
    say "boiling"
}
```

The condition is any expression. It is evaluated using Forge's [truthiness rules](../expressions/comparison.md#truthiness-rules): `false` and `null` are falsy, everything else is truthy.

## If-Else

An `else` clause provides an alternative block when the condition is falsy. Forge supports three equivalent keywords for the else clause:

### Classic: `else`

```forge
if age >= 18 {
    say "adult"
} else {
    say "minor"
}
```

### Natural: `otherwise`

```forge
if age >= 18 {
    say "adult"
} otherwise {
    say "minor"
}
```

### Casual: `nah`

```forge
if age >= 18 {
    say "adult"
} nah {
    say "minor"
}
```

All three forms are semantically identical. The else block executes when the condition is falsy.

## If-Else If Chains

Multiple conditions can be tested in sequence using `else if` (or `otherwise if` / `nah if`):

```forge
if score >= 90 {
    say "A"
} else if score >= 80 {
    say "B"
} else if score >= 70 {
    say "C"
} else {
    say "F"
}
```

Conditions are tested top to bottom. The first truthy condition's block is executed. If no condition is truthy and an `else` clause is present, its block executes.

## Nested If Statements

If statements can be nested arbitrarily:

```forge
if user != null {
    if user.role == "admin" {
        say "admin access"
    } else {
        say "regular access"
    }
} else {
    say "not logged in"
}
```

## Block Scoping

Variables declared inside an `if` or `else` block are scoped to that block:

```forge
if true {
    let msg = "inside"
    say msg     // "inside"
}
// msg is not accessible here
```

## No Ternary Operator

Forge does not have a ternary conditional operator (`condition ? a : b`). Use an `if`-`else` statement or a `when` expression instead:

```forge
// Using when as a conditional expression
let label = when age {
    >= 18 -> "adult",
    else -> "minor"
}
```

## If as a Statement

`if` is always a statement in Forge. It does not produce a value that can be used in expression position. To select between values conditionally, use a [`when` expression](../expressions/when-guards.md).

## Truthiness in Conditions

The condition in an `if` statement follows Forge's truthiness rules:

```forge
if 0 {
    say "zero is truthy"        // this executes
}

if "" {
    say "empty string is truthy"    // this executes
}

if null {
    say "unreachable"
} else {
    say "null is falsy"         // this executes
}

if false {
    say "unreachable"
} else {
    say "false is falsy"        // this executes
}
```

See [Truthiness Rules](../expressions/comparison.md#truthiness-rules) for the complete specification.
