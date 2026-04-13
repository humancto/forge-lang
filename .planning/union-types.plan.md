# Plan: 8C.1 — Union types

## Goal

Support union types like `type StringOrInt = String | Int` in the type checker. A value of type `Int` should be assignable to a variable of type `StringOrInt`.

## Current state

`type X = Y | Z` already parses via TypeDef, storing variants as `Variant { name, fields }`. The typechecker stores variant names in `type_defs` but doesn't use them for type checking.

## Approach

### 1. Add `InferredType::Union`

Add `Union(Vec<InferredType>)` variant to `InferredType`.

### 2. Detect union types in collect_definitions

When processing a `TypeDef`, check if all variant names are known type names (Int, Float, String, Bool, Null, or registered struct/type names). If so, register it as a union type alias.

Store in a new `type_aliases: HashMap<String, InferredType>` map. For `type StringOrInt = String | Int`, store `StringOrInt → Union([String, Int])`.

### 3. Resolve type aliases in type_ann_to_inferred

When `TypeAnn::Simple(name)` matches a type alias, return the aliased type (union or otherwise). This also covers 8C.2 (type aliases) for free.

### 4. Update types_compatible for unions

A type `T` is compatible with `Union(variants)` if `T` is compatible with any variant. A `Union` is compatible with another `Union` if every variant in the first is compatible with some variant in the second.

### 5. Display for Union

Format as `String | Int`.

## Edge cases

- Single-variant union: `type X = Int` — just an alias (also covers 8C.2)
- ADT variants with fields: `type Result = Ok(Int) | Err(String)` — not a union type (variants have fields), keep as ADT
- Nested unions: not needed for v1
- Unknown variant names: not all variants are types → keep as ADT, not union

## Files to touch

1. **`src/typechecker.rs`** — add Union variant, type_aliases map, update types_compatible, update type_ann_to_inferred

## Test strategy

- `type SI = String | Int`: Int assignable to SI, String assignable to SI, Bool not assignable
- `type Nullable = String | Null`: String and null both valid
- Single-variant alias: `type ID = Int` — Int assignable to ID
- ADTs with fields not treated as unions

## Rollback

Revert the single file change.
