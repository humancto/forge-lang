# Plan: 8B.3 — Generic struct definitions

## Goal

Make the type checker aware of generic struct type parameters so that `struct Pair<T> { first: T, second: T }` has its fields correctly typed when the struct is instantiated.

## Current state

Parser already handles `struct Pair<T> { first: T, second: T }` (8B.1). The typechecker stores struct field names but not their types or type params. Constructor calls are not type-checked against struct field types.

## Approach

### 1. Store struct type params and field types

Update `self.structs` from `HashMap<String, Vec<String>>` (field names only) to `HashMap<String, StructInfo>` where:

```rust
struct StructInfo {
    type_params: Vec<String>,
    fields: Vec<(String, InferredType)>,
}
```

### 2. Update collect_definitions for StructDef

Store both field names and their types, plus the struct's type params.

### 3. Update all struct field access sites

The `structs` map is used in:

- `collect_definitions` (stores field names)
- `check_interface_satisfaction` (checks field/method names)
- Possibly `infer_expr` for constructor calls

Update these to work with the new `StructInfo` type.

### 4. Constructor type checking (stretch)

When a struct is constructed like `Pair(1, 2)`, resolve `T = Int` from the arguments and validate field types. This may already work through the function call path if constructors are registered as functions.

## Edge cases

- Non-generic structs: `type_params` is empty, no resolution needed
- Generic struct with unresolved params: fields stay as `Named("T")`
- Struct field access via dot notation: not checked by current typechecker (out of scope)

## Files to touch

1. **`src/typechecker.rs`** — add StructInfo, update collect_definitions, update check_interface_satisfaction

## Test strategy

- Generic struct parses and stores type params
- Non-generic struct still works
- check_interface_satisfaction still works with new StructInfo

## Rollback

Revert the single file change.
