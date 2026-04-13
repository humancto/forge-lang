# Plan: 8B.2 — Generic type resolution in type checker

## Goal

When a generic function is called, resolve type parameters to concrete types based on the arguments provided. This enables the type checker to infer return types of generic functions.

## Current behavior

```forge
fn identity<T>(x: T) -> T { return x }
let y: String = identity(42)  // No warning — T is Unknown
```

## Desired behavior

```forge
fn identity<T>(x: T) -> T { return x }
let y: String = identity(42)  // Warning: expected String but got Int
```

## Approach

### 1. Store type_params in FnSignature

Add `type_params: Vec<String>` to `FnSignature` so the checker knows which function params are generic.

### 2. Resolve type params at call sites

In `infer_expr` for `Expr::Call`, when calling a function with type_params:

1. Infer the type of each argument
2. Build a substitution map: for each generic param, find which function param uses it and bind it to the argument's inferred type
3. Apply the substitution to the return type

### 3. Substitution logic

`resolve_type(ty: &InferredType, substitutions: &HashMap<String, InferredType>) -> InferredType`:

For `InferredType::Named(name)` — if `name` is in the substitution map, replace with the concrete type. Recurse into Array, Option, Result, Function types.

Type param names like "T", "U" are stored as `TypeAnn::Simple("T")` → `InferredType::Named("T")`. When a function has `type_params: ["T"]`, and param `x: T`, the param type is `InferredType::Named("T")`. At a call site `identity(42)`, we know arg 0 is `Int`, so `T = Int`, and return type `Named("T")` resolves to `Int`.

### 4. Update collect_definitions

Store `type_params` from the AST into `FnSignature`.

### 5. Argument type checking with generics

When checking argument types for a generic function, apply the same substitutions so that `fn f<T>(a: T, b: T)` called as `f(1, "hello")` warns about T being both Int and String.

## Edge cases

- No type params → existing behavior (empty substitution map)
- Type param not constrained by any argument → stays Unknown
- Multiple arguments constrain same type param to different types → use first binding (warn on conflict if feasible, but not required for v1)
- Recursive generic calls → substitution is per-call-site, no issue
- Nested generics like `[T]` → resolve recursively

## Files to touch

1. **`src/typechecker.rs`** — update FnSignature, collect_definitions, infer_expr for calls

## Test strategy

- `fn identity<T>(x: T) -> T` called with Int → return type is Int
- `fn first<T, U>(a: T, b: U) -> T` called with (Int, String) → return type is Int
- `fn wrap<T>(x: T) -> [T]` → return type is [Int] when called with Int
- Generic function with no annotations → still works (Unknown args)
- Non-generic functions → no change in behavior

## Rollback

Revert the single file change.
