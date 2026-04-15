# Plan: 8A.1 — Bidirectional type inference for function return types

## Goal

When a function has no explicit return type annotation, infer it from the types of all return statements in the body. Register the inferred return type in `FnSignature` so callers can benefit from it.

## Current behavior

```forge
fn add(a: Int, b: Int) {
    return a + b
}
let x = add(1, 2)  // x is Unknown — checker doesn't know add() returns Int
```

## Desired behavior

```forge
fn add(a: Int, b: Int) {
    return a + b
}
let x = add(1, 2)  // x is Int — inferred from add()'s return statements
```

## Approach

### Two-pass function checking

Currently `collect_definitions` (first pass) registers function signatures with `return_type: None` for unannotated functions. The second pass checks bodies but doesn't feed return type information back into `FnSignature`.

Change:

1. **In `check_stmt` for `Stmt::FunctionDef`**: When `return_type` is `None`, collect all return types from the body. If they all agree (or are compatible), set the inferred return type on the `FnSignature` in `self.functions`.
2. **Add `collect_return_types` helper**: Walk a function body, find all `Stmt::Return(Some(expr))`, infer each expr's type, unify them.
3. **Unification**: If all return types are the same → use that type. If mixed (e.g., `Int` and `Float`) → use `Float` (numeric promotion). If genuinely incompatible → leave as `Unknown`.
4. **Update `infer_expr` for function calls**: Already looks up `FnSignature.return_type` — will automatically benefit from the inferred type.

### Edge cases

- Functions with no return statements → return type is `Null`
- Functions with `return` (no value) → `Null`
- Mixed `return expr` and bare `return` → `Unknown` (ambiguous)
- Recursive functions → leave as `Unknown` (no fixpoint iteration in v1)
- Implicit last-expression returns → not handled yet (would require walking the last statement)

## Files to touch

1. **`src/typechecker.rs`** — add `collect_return_types()`, update function checking logic, update signature after body check

## Test strategy

- Unit tests within `typechecker.rs` for:
  - Simple return type inference (`fn add(a: Int, b: Int) { return a + b }` → `Int`)
  - Multiple consistent returns → same type
  - Mixed numeric returns → `Float`
  - No return → `Null`
  - Incompatible returns → `Unknown`
- Existing tests must continue to pass

## Rollback

Revert the single file change.
