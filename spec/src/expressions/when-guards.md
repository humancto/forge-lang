# When Guards

A **when expression** performs multi-way branching on a scrutinee value using comparison operators. Each arm specifies a comparison operation applied to the scrutinee; the first matching arm's result expression is returned.

## Syntax

```
when expression {
    op value -> result,
    op value -> result,
    else -> result
}
```

The **scrutinee** is the expression after `when`. Each **arm** consists of a comparison operator, a value to compare against, the `->` arrow, and a result expression. The optional `else` arm matches when no other arm does.

## Basic Usage

```forge
let label = when age {
    < 13 -> "child",
    < 18 -> "teen",
    < 65 -> "adult",
    else -> "senior"
}
say label
```

The scrutinee `age` is evaluated once. Each arm's operator and value are applied to the scrutinee in order. The first arm whose comparison returns `true` provides the result.

## Comparison Operators in Arms

Arms support any comparison operator:

| Operator | Meaning               |
| -------- | --------------------- |
| `<`      | Less than             |
| `>`      | Greater than          |
| `<=`     | Less than or equal    |
| `>=`     | Greater than or equal |
| `==`     | Equal                 |
| `!=`     | Not equal             |

```forge
let status = when code {
    == 200 -> "ok",
    == 404 -> "not found",
    == 500 -> "server error",
    >= 400 -> "client error",
    else -> "unknown"
}
```

## Evaluation Semantics

1. The scrutinee expression is evaluated exactly once.
2. Arms are tested top to bottom.
3. For each arm, the arm's comparison operator is applied with the scrutinee as the left operand and the arm's value as the right operand.
4. The first arm that produces `true` determines the result: its result expression is evaluated and returned.
5. If no arm matches and an `else` arm is present, the `else` result is returned.
6. If no arm matches and no `else` arm is present, the when expression evaluates to `null`.

## When as an Expression

`when` produces a value and can be used anywhere an expression is expected:

```forge
say when score {
    >= 90 -> "A",
    >= 80 -> "B",
    >= 70 -> "C",
    else -> "F"
}

let discount = when items {
    > 100 -> 0.20,
    > 50 -> 0.10,
    > 10 -> 0.05,
    else -> 0.0
}
```

## When as a Statement

`when` can also appear at the statement level:

```forge
when temperature {
    > 100 -> say "boiling",
    < 0 -> say "freezing",
    else -> say "normal"
}
```

## Arm Result Expressions

Each arm's result is a single expression. For multi-statement logic, use a block expression or call a function:

```forge
let result = when level {
    > 10 -> {
        let bonus = level * 2
        bonus + 100
    },
    else -> 0
}
```

## The else Arm

The `else` arm is a catch-all that matches when no other arm does. It must be the last arm if present.

```forge
let kind = when x {
    > 0 -> "positive",
    < 0 -> "negative",
    else -> "zero"
}
```

If no `else` arm is provided and no arm matches, the when expression evaluates to `null`.

## Differences from Match

`when` guards and `match` expressions serve different purposes:

| Feature          | `when`                    | `match`                     |
| ---------------- | ------------------------- | --------------------------- |
| Comparison style | Operator-based guards     | Structural pattern matching |
| Scrutinee        | Compared via operators    | Destructured via patterns   |
| Use case         | Numeric/comparable ranges | ADT variant matching        |

See [Match Expressions](./match.md) for structural pattern matching.
