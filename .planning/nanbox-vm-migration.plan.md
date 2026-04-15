# Plan: NaN-boxed Value Migration for VM

## Roadmap Item

`- [ ] NaN-boxed value representation (uniform 64-bit)` (Milestone 2 Deliverables)

## Context

PR #91 landed `src/vm/nanbox.rs` with the full NaN-boxed encoding/decoding module.
The VM still uses `enum Value { Int(i64), Float(f64), Bool(bool), Null, Obj(GcRef) }` (16 bytes).
This plan migrates the VM's `Value` type to use `NanBoxedValue` (8 bytes) as its internal representation.

## Scope

~661 occurrences of `Value::Int/Float/Bool/Null/Obj` across 6 files (machine.rs, value.rs, builtins.rs, compiler.rs, gc.rs, jit/).

## Expert Review Findings (Addressed)

Three showstoppers from the rust-expert review, all addressed below:

- **S1: BoxedInt invisible to extraction API** — `NanBoxedValue::as_int()` only checks inline tag, not heap BoxedInt. Fixed by two-tier extraction API on the `Value` newtype.
- **S2: `Value::int(n)` needs `&mut Gc` for large integers** — Not a simple constructor replacement. Fixed by splitting into `Value::small_int(n)` (panics on overflow) and `Value::int(n, gc)` (handles BoxedInt).
- **S3: Arithmetic overflow semantics** — Three-tier overflow: 48-bit inline → BoxedInt → f64 promotion. Explicitly handled in `arith_op`.

## Strategy: Newtype Wrapper with Two-Tier Int API

### Core Type

```rust
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Value(pub(crate) NanBoxedValue);
```

### Constructor API

| Old               | New (small/known)      | New (arbitrary)          |
| ----------------- | ---------------------- | ------------------------ |
| `Value::Int(42)`  | `Value::small_int(42)` | `Value::int(n, &mut gc)` |
| `Value::Float(f)` | `Value::float(f)`      | same                     |
| `Value::Bool(b)`  | `Value::bool(b)`       | same                     |
| `Value::Null`     | `Value::null()`        | same                     |
| `Value::Obj(r)`   | `Value::obj(r)`        | same                     |

- `Value::small_int(n)` — panics if n doesn't fit in 48 bits. Use for known-small values (0, 1, -1, len, index, bool-to-int).
- `Value::int(n, gc)` — tries inline, falls back to `gc.alloc(ObjKind::BoxedInt(n))`. Use for user input, arithmetic results, constants.

### Extractor API

| Old                        | New                              |
| -------------------------- | -------------------------------- |
| `Value::Int(n)` in match   | `val.as_int(&gc) -> Option<i64>` |
| `Value::Float(f)` in match | `val.as_float() -> Option<f64>`  |
| `Value::Bool(b)` in match  | `val.as_bool() -> Option<bool>`  |
| `val == Value::Null`       | `val.is_null() -> bool`          |
| `Value::Obj(r)` in match   | `val.as_obj() -> Option<GcRef>`  |

**Critical:** `as_int(&gc)` checks BOTH inline NaN-boxed int AND `ObjKind::BoxedInt`:

```rust
pub fn as_int(&self, gc: &Gc) -> Option<i64> {
    if let Some(n) = self.0.as_int() { return Some(n); }
    if let Some(r) = self.0.as_obj() {
        if let Some(obj) = gc.get(r) {
            if let ObjKind::BoxedInt(n) = &obj.kind { return Some(*n); }
        }
    }
    None
}
```

### Exhaustive Matching: `Value::classify()`

For sites that need exhaustiveness guarantees (currently using `match val { ... }`), provide:

```rust
pub enum ValueKind { Int(i64), Float(f64), Bool(bool), Null, Obj(GcRef) }

impl Value {
    pub fn classify(&self, gc: &Gc) -> ValueKind { ... }
}
```

This reconstructs the old enum on-demand. Use at switch sites. Hot paths use direct extractors.

### Three-Tier Arithmetic Overflow

In `arith_op` (machine.rs:2409+):

```
1. checked_add/sub/mul succeeds AND fits 48 bits → Value::small_int(result)
2. checked_add/sub/mul succeeds but > 48 bits → Value::int(result, gc) [BoxedInt on heap]
3. checked_add/sub/mul overflows i64 → Value::float(a as f64 OP b as f64)
```

### Equality: BoxedInt Cross-Comparison

`Value::equals(&self, other: &Value, gc: &Gc)` must handle:

- inline int == inline int (bit pattern match)
- inline int == BoxedInt (extract both as i64, compare)
- BoxedInt == BoxedInt (extract both as i64, compare)
- inline int == float (int as f64 == float)
- BoxedInt == float (int as f64 == float)

## Step-by-Step Plan

### Step 1: Extend NanBoxedValue + Value newtype (single compilable commit)

In `nanbox.rs`:

- No changes needed — existing API is sufficient for inline values

In `value.rs`:

- Replace `pub enum Value` with `#[repr(transparent)] pub struct Value(pub(crate) NanBoxedValue)`
- Add ALL constructor methods (`small_int`, `int`, `float`, `bool`, `null`, `obj`)
- Add ALL extractor methods (`as_int(&gc)`, `as_float()`, `as_bool()`, `is_null()`, `as_obj()`)
- Add `classify(&self, gc: &Gc) -> ValueKind` enum
- Add `is_truthy`, `type_name`, `display`, `equals`, `to_json_string` delegating to NanBoxedValue but with BoxedInt awareness
- Update `value_to_shared` to use classify()
- Update `shared_to_value` to use `Value::int(n, gc)`
- Add `PartialEq` impl that does bitwise comparison (for non-GC contexts like constant dedup)
- Add `#[allow(dead_code)]` shim fns for the OLD enum variant names that delegate to new API (temporary, for compilation during migration)
- **Key: this commit must compile and pass all tests** by keeping the old API surface alive via shims

### Step 2: Migrate machine.rs

- Replace all `Value::Int(n)` constructors → `Value::small_int(n)` or `Value::int(n, &mut self.gc)`
- Replace all pattern matches → extractor methods or `classify()`
- Update `arith_op` with three-tier overflow
- Update GC root scanning to use `val.as_obj()`
- Update constant loading (`Constant::Int(n) => Value::int(*n, &mut self.gc)`)
- **Must compile and pass tests after this commit**

### Step 3: Migrate builtins.rs

- Same mechanical replacement as Step 2
- Integer arithmetic sites use `Value::int(n, gc)` for results
- Type checking sites use `as_int(&gc)` with BoxedInt awareness
- **Must compile and pass tests after this commit**

### Step 4: Migrate compiler.rs, gc.rs, frame.rs

- `compiler.rs`: Constant pool construction uses new API
- `gc.rs`: Mark phase uses `val.as_obj()` for root tracing
- `frame.rs`: Stores `Vec<Value>`, Copy already works — minimal changes
- `ObjUpvalue { value: Value }` trace path updated
- **Must compile and pass tests after this commit**

### Step 5: Migrate serialize.rs + bump format version

- Bump `VERSION_MINOR` to indicate new format
- Serialize constants as NaN-boxed 8-byte words (simpler than tagged enum)
- Reject old-format `.fgc` files with clear error message (no backward compat — format is internal)
- **Must compile and pass tests after this commit**

### Step 6: Migrate JIT files + remove shims

- `type_analysis.rs`: Update Value extraction (handle BoxedInt in JIT eligibility check)
- `runtime.rs`: Update Value extraction for JIT bridges
- **Remove temporary compatibility shims** from Step 1
- **Must compile and pass tests after this commit**

### Step 7: Tests + assertions + cleanup

- `assert_eq!(std::mem::size_of::<Value>(), 8)` — verify size reduction
- Add BoxedInt arithmetic boundary tests (48-bit overflow → heap, i64 overflow → f64)
- Add BoxedInt equality cross-tests (inline vs boxed)
- Run parity tests, examples, full suite
- Remove any dead code from old Value enum

## Edge Cases

- **i64::MAX / i64::MIN**: `Value::int(i64::MAX, gc)` → BoxedInt on heap. `as_int(&gc)` extracts it correctly.
- **Pattern exhaustiveness lost**: Mitigated by `classify()` method returning a matchable enum.
- **GC roots**: `val.as_obj()` returns `Some(GcRef)` for both regular objects AND BoxedInts — both are GC-allocated and need tracing.
- **Serialization**: Bump version, reject old format. No backward compat needed (internal format).
- **Hash for NaN-boxed floats**: `0.0` and `-0.0` have different bits but are equal. Canonicalize in Hash impl (both map to `0u64`). Only needed for constant dedup.
- **ObjUpvalue**: Contains a `Value` field, trace path must use `val.as_obj()`.
- **SharedValue conversion**: `shared_to_value` uses `Value::int(n, gc)` for `SharedValue::Int(n)`.

## Rollback Plan

The old `Value` enum is in git. If this migration breaks things badly, revert the branch.

## Test Strategy

- All existing 948+ Rust tests must pass at EVERY commit
- All parity tests must pass
- Examples must run identically (`hello.fg`, `functional.fg`)
- New tests: BoxedInt overflow boundaries, cross-type equality, size assertions
- Run `forge test` for Forge-level integration tests

## Files Touched (ordered by risk)

1. `src/vm/value.rs` — core type redefinition + extractors
2. `src/vm/nanbox.rs` — minor (already complete)
3. `src/vm/machine.rs` — largest consumer (2495 lines)
4. `src/vm/builtins.rs` — second largest (3516 lines)
5. `src/vm/compiler.rs` — constant pool
6. `src/vm/gc.rs` — mark phase
7. `src/vm/frame.rs` — minimal
8. `src/vm/serialize.rs` — binary format + version bump
9. `src/vm/jit/type_analysis.rs` — minor
10. `src/vm/jit/runtime.rs` — minor

## Estimated Commits

1. `refactor(vm): redefine Value as NanBoxedValue newtype with compat shims`
2. `refactor(vm): migrate machine.rs to new Value API`
3. `refactor(vm): migrate builtins.rs to new Value API`
4. `refactor(vm): migrate compiler.rs, gc.rs, frame.rs`
5. `refactor(vm): update serialize.rs + bump format version`
6. `refactor(vm): migrate JIT files + remove compat shims`
7. `test(nanbox): BoxedInt boundary tests + size assertion`
