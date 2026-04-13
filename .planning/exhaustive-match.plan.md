# Plan: 8A.3 — Exhaustive match checking

## Goal

Warn when a `match` expression doesn't cover all variants of a known type. Essential for Result/Option patterns to catch missed error cases.

## Current behavior

```forge
let result: Result<Int, String> = Ok(42)
match result {
    Ok(v) => { say v }
}
// No warning — missing Err arm silently ignored
```

## Desired behavior

```forge
match result {
    Ok(v) => { say v }
}
// Warning: non-exhaustive match on Result<Int, String> — missing: Err
```

## Approach

### 1. After checking all match arms, analyze exhaustiveness

In `check_stmt` for `Stmt::Match`, after the existing arm-checking loop:

1. Determine the subject type
2. Compute the set of required variants for that type
3. Compute the set of covered variants from the arm patterns
4. If there's a wildcard (`_`) or binding pattern, the match is exhaustive
5. Otherwise, warn about missing variants

### 2. Known exhaustible types

- **`Option<T>`**: requires `Some(...)` + `None`/`null` (or wildcard)
- **`Result<T, E>`**: requires `Ok(...)` + `Err(...)` (or wildcard)
- **`Bool`**: requires `true` + `false` (or wildcard)

For all other types (Int, String, Named structs, Unknown), exhaustiveness cannot be statically checked — skip silently.

### 3. Pattern coverage extraction

Walk the arms and collect which variants are covered:

```rust
enum CoveredVariant {
    Some,
    None,   // Pattern::Literal(Expr::Ident("null"/"None")) or Pattern::Binding("null"/"None")
    Ok,
    Err,
    True,
    False,
    Wildcard,  // Pattern::Wildcard or Pattern::Binding(_)
}
```

A `Pattern::Binding` that isn't "null"/"None" acts as a wildcard (it matches anything).

### 4. Emit warnings

Format: `"non-exhaustive match on {type} — missing: {variants}"`

Only emit when:

- The subject type is a known exhaustible type
- No wildcard/binding pattern is present
- Some required variants are missing

## Edge cases

- `match x { _ => { } }` — always exhaustive (wildcard)
- `match x { v => { } }` — always exhaustive (binding)
- Bool match with only `true` → warn about missing `false`
- Option with only `Some` → warn about missing `None`
- Unknown subject type → no exhaustiveness check
- Named types that happen to be enums → not checked (would need enum variant registry, out of scope)

## Files to touch

1. **`src/typechecker.rs`** — add exhaustiveness check after match arm loop

## Test strategy

- Option match missing None → warns
- Option match with Some + None → no warning
- Result match missing Err → warns
- Result match with Ok + Err → no warning
- Bool match missing false → warns
- Wildcard makes any match exhaustive
- Binding pattern makes any match exhaustive
- Unknown type → no warning
- Non-exhaustible type (Int) → no warning

## Rollback

Revert the single file change.
