# Plan: 11A.1 — Intern Strings in GC

## Goal

Deduplicate identical strings via hash-consing in the GC. Identical strings share a single allocation, reducing memory and enabling fast equality checks (11A.2).

## Current State

- Strings are `ObjKind::String(String)` in `GcObject`, allocated via `gc.alloc_string(s.to_string())`
- Every string allocation creates a new `GcObject`, even for duplicates
- `constant_to_value()` allocates a fresh string for every `LoadConst` of a string literal
- String equality uses `a == b` on the underlying Rust `String`

## Design

### Intern table in GC

Add a `HashMap<String, GcRef>` to `Gc` that maps string content to its canonical `GcRef`. When `alloc_string` is called:

1. Check the intern table for an existing entry
2. If found, return the existing `GcRef` (no allocation)
3. If not found, allocate a new `GcObject`, insert into intern table, return the ref

### GC sweep interaction

During sweep, interned strings that are unreachable must be removed from the intern table too. In the `sweep()` phase, when freeing a `GcObject` that is a `String`, remove it from the intern table.

### Implementation

1. **`src/vm/gc.rs`**:
   - Add `interned: HashMap<String, GcRef>` field to `Gc`
   - Modify `alloc_string()` to check intern table first
   - Add `intern_string()` method (explicit interning)
   - Modify `sweep()` to clean up intern table entries for freed strings
   - Initialize `interned` in `Gc::new()`

2. **`src/vm/value.rs`**: No changes needed — `ObjKind::String(String)` stays the same. Interning is transparent.

3. **`src/vm/machine.rs`**: No changes needed for 11A.1 — `alloc_string()` callers automatically benefit from interning since the GC method is the single entry point.

### What NOT to do in 11A.1

- Short string optimization (inline ≤23 bytes) — too invasive for this item, would require changing `Value` size
- Pointer-based equality (11A.2)
- Field name interning (11A.3)

## Edge Cases

- Empty string: interned like any other
- Strings built by concatenation: interned after construction
- Strings from external sources (file reads, HTTP responses): interned on allocation
- GC pressure from intern table itself: HashMap entries are overhead, but net positive since we eliminate duplicate GcObjects

## Test Strategy

- Same string literal loaded twice → same `GcRef`
- Interned string survives GC when referenced
- Unreferenced interned string is collected and removed from table
- Concatenation producing an already-interned string → reuses existing ref
- All existing tests pass (interning should be transparent)

## Rollback

Revert changes to `src/vm/gc.rs`.
