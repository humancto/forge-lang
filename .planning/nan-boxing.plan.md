# Plan: NaN-Boxed Value Representation

## Goal

Replace the current 16-byte `enum Value` with an 8-byte NaN-boxed `u64`, halving memory for registers, stacks, arrays, and objects. This is the foundation for Milestone 2 performance targets.

## Background

IEEE 754 doubles use a quiet NaN pattern when all 11 exponent bits are 1 and the mantissa is non-zero. This leaves ~51 bits of payload for encoding non-float values. The current `Value` enum is 16 bytes (8-byte tag + 8-byte payload); NaN-boxing compresses it to 8 bytes.

## Encoding Scheme

```
Float:   any f64 that is NOT a quiet NaN with our tag bits
         (normal numbers, infinities, and the canonical NaN 0x7FF8000000000000)

Quiet NaN space (exponent=0x7FF, bit 51=1):
  Bit layout: [sign:1][exp:11][quiet:1][tag:3][payload:48]

Tag bits (bits 48-50):
  000 = Null      payload = 0
  001 = Bool      payload = 0 or 1
  010 = Int       payload = 48-bit signed int (sign-extended)
  011 = Obj       payload = 48-bit GcRef index
  1xx = reserved for future (Option, Result, etc.)
```

Full 64-bit integers that don't fit in 48 bits: box as ObjKind::BoxedInt(i64) on the heap. This is rare — only values > ±140 trillion.

## Implementation Strategy

### Phase 1: New NanBoxedValue type (additive, no breakage)

Create `src/vm/nanbox.rs` with:

- `#[derive(Clone, Copy, PartialEq)] pub struct NanBoxedValue(u64)`
- Constructors: `from_float(f64)`, `from_int(i64)`, `from_bool(bool)`, `null()`, `from_obj(GcRef)`
- Extractors: `as_float() -> Option<f64>`, `as_int() -> Option<i64>`, `is_null()`, `as_obj() -> Option<GcRef>`, etc.
- Type query: `is_float()`, `is_int()`, `is_bool()`, `is_null()`, `is_obj()`
- Constants: `QNAN`, `TAG_MASK`, `SIGN_BIT`, tag patterns
- Comprehensive unit tests for every encoding/decoding path, edge cases (NaN, infinity, zero, -0, MAX_INT, overflow to boxed)

### Phase 2: Type alias swap

- `pub type Value = NanBoxedValue;` in value.rs
- Update all `Value::Int(n)` → `Value::from_int(n)`, `Value::Float(f)` → `Value::from_float(f)`, etc.
- Update all pattern matches: `Value::Int(n) => ...` → `val.as_int() => Some(n) => ...` or use helper methods
- This is the bulk of the work — ~700 match sites across builtins.rs and machine.rs

### Phase 3: Adapt dependent systems

- GC: trace/sweep use GcRef — just extract via `as_obj()`
- Compiler: `Constant` enum stays the same, but `Value` construction changes
- JIT: already uses i64 for int values — compatible
- SharedValue: conversion functions update
- Serialization: handle boxed ints in bytecode format

## Risk Mitigation

- Phase 1 is purely additive — zero risk to existing code
- Phase 2 uses the type alias so ALL existing code must compile before tests run
- Integer overflow (>48 bits): implement ObjKind::BoxedInt with transparent unwrap
- NaN canonicalization: any arithmetic producing NaN must produce the canonical NaN (0x7FF8000000000000), not a tagged NaN

## Test Strategy

Phase 1 tests (in nanbox.rs):

1. Round-trip every value type (float, int, bool, null, obj)
2. Edge cases: f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 0.0
3. Int boundary: i48::MAX, i48::MIN, overflow to boxed
4. GcRef with large indices
5. Type discrimination: is_float/is_int/is_bool/is_null/is_obj are mutually exclusive

Phase 2 tests: existing 1145 tests must all pass unchanged.

## Files to touch

1. NEW: `src/vm/nanbox.rs` — NaN-boxing implementation + tests
2. `src/vm/mod.rs` — add `mod nanbox`
3. `src/vm/value.rs` — type alias, update methods
4. `src/vm/builtins.rs` — update ~410 match sites
5. `src/vm/machine.rs` — update ~191 match sites
6. `src/vm/gc.rs` — minor updates
7. `src/vm/compiler.rs` — constant construction
8. `src/vm/jit/runtime.rs` — bridge functions

## Scope

This plan covers Phase 1 only (create nanbox.rs with full tests). Phase 2 (swap) is a separate roadmap item due to its size.

Actually, given the massive scope of Phase 2 (~700 match sites), let me reconsider. The roadmap item says "NaN-boxed value representation" — the deliverable is the full swap. But we should break this into commits:

1. Commit 1: nanbox.rs with encoding/decoding + exhaustive unit tests
2. Commit 2: Value type alias + method adapters (is_truthy, display, equals, etc.)
3. Commit 3: machine.rs migration
4. Commit 4: builtins.rs migration
5. Commit 5: compiler.rs, gc.rs, serialization updates

Each commit must pass all tests.

## Rollback

Revert nanbox.rs, restore original Value enum.
