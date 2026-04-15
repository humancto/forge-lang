# Plan: 11A.2 — Interned String Comparison

## Goal

`==` on interned strings becomes GcRef pointer comparison instead of byte-by-byte.

## Current State

String equality in `Value::equals()` dereferences both GcRefs, gets the underlying `String`, and does `a == b` (byte comparison). With interning from 11A.1, identical short strings share the same GcRef, so we can short-circuit: if both GcRefs are equal, the values are equal.

## Design

Add a fast path in `Value::equals()`: when comparing two `Value::Obj(a)` and `Value::Obj(b)`, check `a == b` first. If the GcRefs are equal, return `true` immediately without dereferencing.

This is safe because:

1. Interned strings share the same GcRef for identical content
2. Non-interned strings (>128 bytes) still fall through to byte comparison
3. Non-string objects with different refs still fall through to deep comparison

### Implementation

1. **`src/vm/value.rs`** — Add `a == b` fast path at the top of the `(Value::Obj(a), Value::Obj(b))` match arm in `equals()`

That's it. One line change.

### Files to touch

1. **`src/vm/value.rs`** — `Value::equals()` method

## Test strategy

- All existing tests pass (transparent optimization)
- No new tests needed — this is a performance optimization, not a behavior change. The existing equality tests validate correctness.

## Rollback

Revert the one-line change in `src/vm/value.rs`.
