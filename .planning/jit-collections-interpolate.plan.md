# Plan: JIT Handles Strings, Arrays, Objects, Floats

## Roadmap Item

`- [ ] JIT handles strings, arrays, objects, floats` (ROADMAP.md line 242)

## Current State

- NaN-boxing: Done (Phase 2.1)
- String ops (Concat, Len, Eq): Done via runtime bridges
- Interpolate: Marked unsupported in type_analysis
- NewArray, GetIndex, SetIndex: Marked unsupported
- NewObject, GetField, SetField: Marked unsupported
- ExtractField: Marked unsupported
- Mixed string+float: Rejected by type_analysis — **will remain rejected** (see below)
- Float support: Done (pure-float functions compile fine)

## Expert Review Findings (incorporated)

1. **Do NOT lift the string+float restriction.** The ir_builder uses a single `use_float` boolean for all registers (line 107/144). Supporting per-register mixed types requires a rewrite. Out of scope.
2. **Type erasure at bridge boundaries.** Raw i64 values in JIT registers lose type info. Bridges can't distinguish int 42 from GcRef(42). Solution: use tagged encoding at call sites.
3. **GetField/SetField use constant pool indices**, not register GcRefs.
4. **GC safety**: clone GcRef-pointed data before allocating new objects to prevent collection of unrooted refs.
5. **JitEntry metadata** must be updated for collection-returning functions.
6. **Symbol registration** must happen alongside ir_builder wiring, not after.
7. **Call opcode** in collection-mode functions must pass vm_ptr.

## Design

### Tagged encoding at bridge boundaries

The type_analysis pass already tracks per-register types (`RegType::Int`, `StringRef`, `Bool`, `Unknown`). Before calling a bridge that accepts heterogeneous values (array creation, interpolation), the JIT emits tag-encoding instructions:

```
encoded = (tag << 60) | (payload & 0x0FFF_FFFF_FFFF_FFFF)
```

Tags: 0=Int, 1=Float, 2=Bool, 3=Null, 4=Obj (matching existing constants in runtime.rs).

The bridge decodes tagged values to `Value` using the existing `decode_value()` function. Return values from bridges are also tagged; the JIT emits decoding instructions based on the expected return type.

For simple bridges where both sides know the type (e.g., `rt_array_get` always returns tagged, `rt_string_len` always returns raw i64), we skip encoding/decoding where unnecessary.

### Scope: integer-mode functions only

Collection ops only compile in "integer mode" (`use_float == false`). Functions that mix floats with collections are rejected. This matches the existing string ops constraint.

### vm_ptr unification

The ir_builder creates `vm_ptr_var` and adds vm_ptr as the first function parameter when `has_string_ops` is true. This must be extended: create vm_ptr when `has_string_ops || has_collection_ops`. All bridge call sites and the Call opcode check the same unified condition. Max arity is 7 (not 8) when vm_ptr is present, matching the existing string ops behavior.

### Len bridge unification

The `rt_string_len` bridge import (currently guarded by `has_string_ops`) is replaced with `rt_obj_len` imported when `has_string_ops || has_collection_ops`. This handles String, Array, and Object lengths.

### JitEntry return type tracking

`JitEntry` currently tracks `returns_string: bool`. This is replaced with a `return_kind: JitReturnKind` enum: `Int`, `StringRef`, `ObjRef`. The dispatch path uses this to correctly wrap the i64 return value as `Value::int(...)`, `Value::obj(GcRef(...))`, etc.

### Zero-count collections

Cranelift may reject `create_sized_stack_slot` with size 0. For NewArray/NewObject/Interpolate with count=0, skip the stack allocation and bridge call entirely — emit a direct call to a zero-arg bridge (`rt_empty_array`, `rt_empty_object`, `rt_empty_string`) or hardcode the allocation inline.

## Implementation Plan

### Commit 1: type_analysis — track collection ops + new RegType variants

**File: `src/vm/jit/type_analysis.rs`**

- Add `RegType::ObjRef` variant for registers holding array/object GcRefs
- Remove `NewArray`, `NewObject`, `GetField`, `SetField`, `GetIndex`, `SetIndex`, `Interpolate`, `ExtractField` from the unsupported ops list
- Track these ops with a new flag `has_collection_ops: bool`
- `NewArray` → destination is `ObjRef`
- `NewObject` → destination is `ObjRef`
- `GetField` → destination is `Unknown` (could be any type)
- `GetIndex` → destination is `Unknown`
- `SetField`/`SetIndex` → no destination type change
- `Interpolate` → destination is `StringRef`
- `ExtractField` → destination is `Unknown`
- Reject if `has_collection_ops && has_float` (same as string restriction)
- Add `has_collection_ops` flag to `TypeInfo`
- Add tests for new analysis paths

### Commit 2: runtime bridges for arrays (`runtime.rs`) + symbol registration (`jit_module.rs`)

**File: `src/vm/jit/runtime.rs`**

```rust
// Takes a pointer to tagged-encoded i64 values, returns GcRef index as i64
pub extern "C" fn rt_array_new(vm_ptr: *mut VM, elements_ptr: *const i64, count: i64) -> i64

// arr_ref is raw GcRef index, idx is raw i64. Returns tagged value.
pub extern "C" fn rt_array_get(vm_ptr: *mut VM, arr_ref: i64, idx: i64) -> i64

// arr_ref is raw GcRef index, idx is raw i64, val is tagged.
pub extern "C" fn rt_array_set(vm_ptr: *mut VM, arr_ref: i64, idx: i64, val: i64)

// Returns raw i64 length (not tagged — caller knows it's an int)
pub extern "C" fn rt_array_len(vm_ptr: *mut VM, arr_ref: i64) -> i64
```

GC safety in `rt_array_new`: collect all tagged values into a `Vec<Value>` first (decoding tags), then call `gc.alloc(ObjKind::Array(values))`. The decode step doesn't allocate, so no GC can fire between reading the elements and creating the array.

**File: `src/vm/jit/jit_module.rs`**

Register `rt_array_new`, `rt_array_get`, `rt_array_set`, `rt_array_len` symbols in the JIT builder alongside existing string bridges.

### Commit 3: runtime bridges for objects + symbol registration

**File: `src/vm/jit/runtime.rs`**

```rust
// pairs_ptr points to tagged [key, val, key, val, ...], returns GcRef index
pub extern "C" fn rt_object_new(vm_ptr: *mut VM, pairs_ptr: *const i64, pair_count: i64) -> i64

// obj_ref is GcRef index, field_const_idx is constant pool index. Returns tagged value.
pub extern "C" fn rt_object_get(vm_ptr: *mut VM, obj_ref: i64, field_const_idx: i64) -> i64

// obj_ref is GcRef index, field_const_idx is constant pool index, val is tagged.
pub extern "C" fn rt_object_set(vm_ptr: *mut VM, obj_ref: i64, field_const_idx: i64, val: i64)

// field_index is the numeric suffix (0 for "_0", 1 for "_1"). Returns tagged value.
pub extern "C" fn rt_extract_field(vm_ptr: *mut VM, obj_ref: i64, field_index: i64) -> i64
```

**Important**: `rt_object_get`/`rt_object_set` receive a **constant pool index** (not a GcRef). The bridge reads the chunk's constant pool to get the field name string. This requires passing a chunk pointer — add `chunk_ptr: *const Chunk` parameter OR pre-intern the field name as a GcRef and pass that instead.

**Decision**: Pre-intern field name constants as GcRefs (same as string constants). The JIT loads the GcRef index as an i64 constant and passes it to the bridge. The bridge reads the string from the GC. This is consistent with how string constants work.

**File: `src/vm/jit/jit_module.rs`** — register object bridge symbols.

### Commit 4: runtime bridge for interpolate + generalized Len

**File: `src/vm/jit/runtime.rs`**

```rust
// parts_ptr points to tagged values, returns GcRef index for result string
pub extern "C" fn rt_interpolate(vm_ptr: *mut VM, parts_ptr: *const i64, count: i64) -> i64
```

Decode each tagged value, call `value.display(&gc)`, concatenate, allocate result string.

Generalize `rt_string_len` → `rt_obj_len` that handles String, Array, and Object:

```rust
pub extern "C" fn rt_obj_len(vm_ptr: *mut VM, obj_ref: i64) -> i64
```

**File: `src/vm/jit/jit_module.rs`** — register symbols.

### Commit 5: ir_builder — compile array opcodes

**File: `src/vm/jit/ir_builder.rs`**

Import array bridge function refs when `type_info.has_collection_ops`:

- `rt_array_new`, `rt_array_get`, `rt_array_set`, `rt_array_len` (if needed separately)

**NewArray** (A=dst, B=start_reg, C=count):

```
// Stack-allocate buffer for tagged elements
slot = create_sized_stack_slot(count * 8)
for i in 0..count:
    val = use_var(regs[start + i])
    tagged = encode_with_type(val, reg_types[start + i])  // emit tag encoding
    stack_store(tagged, slot, i * 8)
ptr = stack_addr(slot)
count_val = iconst(count)
result = call(rt_array_new, [vm_val, ptr, count_val])
def_var(regs[dst], result)
```

**GetIndex** (A=dst, B=obj_reg, C=idx_reg):

```
arr = use_var(regs[obj_reg])
idx = use_var(regs[idx_reg])
tagged_result = call(rt_array_get, [vm_val, arr, idx])
decoded = decode_for_type(tagged_result, reg_types[dst])  // or keep tagged
def_var(regs[dst], decoded)
```

**SetIndex** (A=obj_reg, B=idx_reg, C=val_reg):

```
arr = use_var(regs[obj_reg])
idx = use_var(regs[idx_reg])
val = use_var(regs[val_reg])
tagged_val = encode_with_type(val, reg_types[val_reg])
call(rt_array_set, [vm_val, arr, idx, tagged_val])
```

Tag encoding helper (emitted as Cranelift IR):

```rust
fn emit_tag_encode(b: &mut FunctionBuilder, val: Value, reg_type: RegType) -> Value {
    match reg_type {
        RegType::Int => {
            let tag = b.ins().iconst(I64, TAG_INT << 60);
            let masked = b.ins().band_imm(val, PAYLOAD_MASK as i64);
            b.ins().bor(tag, masked)
        }
        RegType::Bool => {
            let tag = b.ins().iconst(I64, TAG_BOOL << 60);
            b.ins().bor(tag, val)
        }
        RegType::StringRef | RegType::ObjRef => {
            let tag = b.ins().iconst(I64, TAG_OBJ << 60);
            b.ins().bor(tag, val)
        }
        _ => val, // Unknown — pass raw, bridge handles gracefully
    }
}
```

Tag decoding helper:

```rust
fn emit_tag_decode_int(b: &mut FunctionBuilder, tagged: Value) -> Value {
    b.ins().band_imm(tagged, PAYLOAD_MASK as i64)
    // Sign extension if needed for negative ints
}
```

### Commit 6: ir_builder — compile object opcodes

**NewObject** (A=dst, B=start_reg, C=pair_count):

```
slot = create_sized_stack_slot(pair_count * 2 * 8)  // key-value pairs
for i in 0..pair_count:
    key = use_var(regs[start + i*2])       // key is always StringRef
    val = use_var(regs[start + i*2 + 1])
    tagged_key = encode_with_type(key, RegType::StringRef)
    tagged_val = encode_with_type(val, reg_types[start + i*2 + 1])
    stack_store(tagged_key, slot, (i*2) * 8)
    stack_store(tagged_val, slot, (i*2+1) * 8)
ptr = stack_addr(slot)
result = call(rt_object_new, [vm_val, ptr, pair_count])
def_var(regs[dst], result)
```

**GetField** (A=dst, B=obj_reg, C=field_name_const_idx):

```
obj = use_var(regs[obj_reg])
// Load pre-interned GcRef for the field name constant
field_ref = iconst(string_refs[C])  // reuse string constant pre-allocation
tagged_result = call(rt_object_get, [vm_val, obj, field_ref])
decoded = decode_for_type(tagged_result, reg_types[dst])
def_var(regs[dst], decoded)
```

**SetField** (A=obj_reg, B=field_name_const_idx, C=val_reg):

```
obj = use_var(regs[obj_reg])
field_ref = iconst(string_refs[B])
val = use_var(regs[val_reg])
tagged_val = encode_with_type(val, reg_types[val_reg])
call(rt_object_set, [vm_val, obj, field_ref, tagged_val])
```

**ExtractField** (A=dst, B=obj_reg, C=field_index):

```
obj = use_var(regs[obj_reg])
field_idx = iconst(C)
tagged_result = call(rt_extract_field, [vm_val, obj, field_idx])
decoded = decode_for_type(tagged_result, reg_types[dst])
def_var(regs[dst], decoded)
```

### Commit 7: ir_builder — compile Interpolate + update Len + fix Call

**Interpolate** (A=dst, B=start_reg, C=part_count):

```
slot = create_sized_stack_slot(count * 8)
for i in 0..count:
    val = use_var(regs[start + i])
    tagged = encode_with_type(val, reg_types[start + i])
    stack_store(tagged, slot, i * 8)
ptr = stack_addr(slot)
result = call(rt_interpolate, [vm_val, ptr, count_val])
def_var(regs[dst], result)
```

**Len**: Replace `rt_string_len` import with `rt_obj_len` that handles String/Array/Object. Import condition: `has_string_ops || has_collection_ops` (not just `has_string_ops`).

**Call**: When `has_string_ops || has_collection_ops`, the Call opcode must pass `vm_ptr` as the first argument to the self-recursive call. Guard this with a check: only prepend vm_val when `vm_ptr_var.is_some()`. Pure-integer functions must NOT prepend vm_ptr even if they self-recurse.

### Commit 8: JitEntry metadata + dispatch updates

**File: `src/vm/machine.rs`**

Update `JitEntry`:

- Add `has_collection_ops: bool` flag
- Replace `returns_string: bool` with `return_kind: JitReturnKind` enum (`Int`, `StringRef`, `ObjRef`)
- Set `max_arity` to 7 when `has_string_ops || has_collection_ops` (vm_ptr takes one slot)

Update the JIT dispatch path (around line 2200) to:

- Pass vm_ptr when calling JIT functions with `has_string_ops || has_collection_ops`
- Use `return_kind` to correctly wrap the i64 return value: `Int` → `Value::int(...)`, `StringRef` → `Value::obj(GcRef(...))`, `ObjRef` → `Value::obj(GcRef(...))`

### Commit 9: Tests (alongside each commit)

Tests added incrementally with each commit:

- type_analysis tests: collection ops flagged correctly, mixed float+collection rejected
- runtime bridge unit tests: rt_array_new/get/set, rt_object_new/get/set, rt_interpolate
- JIT integration tests:
  - Function creating array [1, 2, 3], returning arr[1] → 2
  - Function creating object {x: 10}, returning obj.x → 10
  - String interpolation `"hello {name}"` via JIT
  - Array length
  - Object field assignment
  - ExtractField from tuple-like object
- Parity tests: same programs via --vm and --jit produce identical results
- All existing tests pass

## Files Modified

1. `src/vm/jit/type_analysis.rs` — new RegType::ObjRef, collection ops tracking
2. `src/vm/jit/runtime.rs` — 7 new bridge functions
3. `src/vm/jit/jit_module.rs` — register all new bridge symbols
4. `src/vm/jit/ir_builder.rs` — compile 8 new opcodes, tag encode/decode helpers
5. `src/vm/machine.rs` — JitEntry metadata, dispatch path updates

## Edge Cases

- Empty arrays (count=0): skip stack alloc, call `rt_empty_array(vm_ptr)` directly
- Empty objects (pair_count=0): skip stack alloc, call `rt_empty_object(vm_ptr)` directly
- GetIndex out of bounds: bridge returns tagged null (TAG_NULL << 60)
- GetField missing field: bridge returns tagged null
- SetIndex on non-array: bridge is a no-op
- Interpolate with 0 parts: bridge returns GcRef to empty string
- GetIndex/GetField return Unknown type: keep tagged, decode only when consumed by typed op

## GC Safety

- All bridge functions that allocate (rt_array_new, rt_object_new, rt_interpolate) first collect all input values into a Rust Vec (no GC refs held), then allocate. This prevents dangling refs during GC.
- The stack buffer contains tagged i64s, not raw GcRefs. The GC doesn't scan the Cranelift stack, but the values are decoded and rooted in the Vec before any allocation.

## What's NOT in scope

- Mixed float+collection functions (requires per-register type in Cranelift — separate milestone)
- Closure/upvalue compilation
- Global variable access from JIT
- Error propagation from bridges (bridges return null on error; VM raises errors for out-of-bounds etc. — known semantic difference, documented)

## Rollback

Revert all commits on the feature branch. No schema or data changes.
