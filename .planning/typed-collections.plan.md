# Plan: 8C.3 — Typed collection literals

## Goal

Validate element types in array literals when a type annotation is present: `let xs: [Int] = [1, 2, 3]` passes, `let xs: [Int] = [1, "two", 3]` warns.

## Approach

### 1. Update infer_expr for Expr::Array

Currently `Expr::Array` returns `InferredType::Unknown`. Infer the element type from the elements: if all elements have the same type, return `Array(that_type)`.

### 2. Update types_compatible for Array

`Array(Int)` is compatible with `Array(Int)`. `Array(Unknown)` is compatible with any `Array(T)`.

### 3. Existing let check handles the rest

The existing `Let` type checking compares `expected` vs `inferred`. If `expected` is `[Int]` and `inferred` is `[String]`, it will warn.

## Files to touch

1. **`src/typechecker.rs`** — update infer_expr for Array, update types_compatible for Array

## Test strategy

- `let xs: [Int] = [1, 2, 3]` — no warning
- `let xs: [Int] = [1, "two", 3]` — warns (inferred as Unknown array)
- `let xs: [String] = ["a", "b"]` — no warning
- Empty array `let xs: [Int] = []` — no warning (empty is compatible)

## Rollback

Revert the single file change.
