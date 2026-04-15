# Plan: JIT handles all function calls (revised after expert review)

## Goal

Enable JIT-compiled functions to call other functions (not just self-recursion) and access globals via runtime bridges.

## Current State

- `OpCode::Call` in ir_builder only emits self-recursive calls via `self_ref`
- `GetGlobal`, `SetGlobal`, `Closure`, `GetUpvalue`, `SetUpvalue` all marked unsupported
- `rt_call_native` bridge exists but unused from ir_builder
- `rt_get_global` exists as a stub (returns null)
- VM `call_value()` checks `jit_cache`, so JIT→bridge→VM→JIT works transparently

## Expert Review Findings (addressed)

- **S1 (&mut VM aliasing):** All bridges create `&mut *vm_ptr` while caller's `&mut self` is live. This is UB by Rust aliasing rules but safe in practice: FFI boundary is opaque to optimizer, and all register state is stored before bridge call. Add SAFETY comments to all bridges. Long-term: restructure with UnsafeCell.
- **S2 (tag-encoding for function values from GetGlobal):** `rt_get_global` returns tagged values. Store as-is in Unknown registers. For Call, pass the tagged value directly to `rt_call_native` without re-encoding.
- **B1 (rt_get_global parameter):** Takes GcRef index from string_refs, resolves to string via gc.get(), then looks up vm.globals.
- **B2 (error propagation):** Bridge calls return encode_null on error. Accept divergence from VM for this phase; document. Error flag approach deferred.
- **B3 (Call destination type):** Change Call destination to `RegType::Unknown` for bridge calls. This correctly rejects functions that return bridge-call results directly (return_type = Unknown → unsupported).
- **R3 (float + global):** `has_global_ops` is a distinct flag. Does NOT trigger the float rejection rule. Globals + float is valid.
- **M2 (SetGlobal encoding):** Bridge receives tagged value via emit_tag_encode with register's actual RegType.

## Design

### Value Flow Convention

- `rt_get_global` returns **tagged** u64 (via encode_value)
- GetGlobal destination register type = `Unknown`
- Unknown registers hold **raw decoded** values (consistent with existing convention)
- For GetGlobal: decode the tagged result via `emit_tag_decode_int` (same as GetField/ExtractField for Unknown)
- For Call bridge: the function register may be Unknown. Tag-encode with `emit_tag_encode(Unknown)` which tags as TAG_INT. But function values are ObjRef!
- **Fix:** Detect when Call's function register came from GetGlobal (type is Unknown). In that case, the register holds a raw decoded value from a tagged bridge return. Tag-encode as TAG_OBJ since globals holding functions are ObjRefs.
- **Simpler fix:** Just tag-encode as TAG_OBJ for the function argument specifically (it's always an object reference), regardless of reg type. The bridge calls `decode_value` which handles any tag correctly.

### Self-call detection

The existing Call handler uses `self_ref` for ALL calls. We need to distinguish:

- **Self-recursive call:** register `a` was loaded with the current function reference → use existing `self_ref` direct call
- **General call:** register `a` holds a different function → use `rt_call_native` bridge

**Heuristic:** Track whether register `a` was set by the function's own parameter loading or self-reference. If the function only has one Call and it's clearly self-recursive (register a matches the closure register), keep self_ref. Otherwise, use bridge.

**Simpler approach:** If function uses GetGlobal (has_global_ops), ALL Call instructions go through the bridge (including self-recursion). If no GetGlobal, keep existing self_ref behavior. This is conservative but correct — functions with globals likely call other functions.

**Even simpler:** Check if the function register `a` is the same register that was loaded via the function's own parameter (register 0 for named functions). If `a == 0` and no GetGlobal was used to write to register 0, it's self-recursive. Otherwise, use bridge.

Actually the simplest correct approach: **always use bridge when `has_global_ops` is true**. When `has_global_ops` is false, the only callable thing in a register is the function itself (loaded at entry), so self_ref works.

### Call destination type

- When using self_ref: destination type = `RegType::Int` (existing behavior, correct for int-returning self-recursion)
- When using bridge: destination type = `RegType::Unknown` (bridge returns tagged, we decode as int, but type_analysis marks Unknown-return functions unsupported)

**Wait — we can't distinguish self vs bridge at type_analysis time.** Type analysis runs before ir_builder. At analysis time, we don't know which calls are self vs bridge.

**Fix:** If `has_global_ops` is true, set ALL Call destinations to `RegType::Unknown`. If false, keep `RegType::Int`. This is conservative: functions with globals that call themselves and return the result will be rejected (return_type = Unknown). That's acceptable — such functions are rare and can fall back to VM.

## Implementation Steps

### 1. type_analysis.rs

- Move `GetGlobal` and `SetGlobal` from unsupported to tracked
- Add `has_global_ops: bool` to TypeInfo
- GetGlobal: `types[a] = RegType::Unknown`, `has_global_ops = true`
- SetGlobal: `has_global_ops = true` (no type change)
- If `has_global_ops`: Call destination = `RegType::Unknown`
- Do NOT add global_ops to the float rejection rule
- Keep Closure/GetUpvalue/SetUpvalue unsupported

### 2. runtime.rs

- Fix `rt_get_global(vm_ptr, name_ref: i64) -> i64`:
  - Resolve GcRef(name_ref) → string via gc.get()
  - Look up vm.globals by string name
  - Return encode_value(&val) or encode_null()
- Add `rt_set_global(vm_ptr, name_ref: i64, val: i64)`:
  - Resolve GcRef(name_ref) → string
  - Decode val via decode_value(val as u64)
  - Insert into vm.globals
- Add SAFETY comments to rt_call_native and all bridges documenting the &mut aliasing situation

### 3. ir_builder.rs

- Import global bridges (get_global, set_global)
- GetGlobal handler:
  - Load string_ref for constant index bx
  - Call rt_get_global(vm_ptr, name_ref)
  - Decode result: Unknown → emit_tag_decode_int (consistent with existing convention)
- SetGlobal handler:
  - Load string_ref for constant index bx
  - Tag-encode source register value via emit_tag_encode with its RegType
  - Call rt_set_global(vm_ptr, name_ref, tagged_val)
- Call handler (when has_global_ops):
  - Tag-encode function register as TAG_OBJ (always an object reference)
  - Stack-allocate buffer for tagged args
  - Tag-encode each arg via emit_tag_encode
  - Call rt_call_native(vm_ptr, func_tagged, args_ptr, argc)
  - Decode result as int (emit_tag_decode_int)
  - Store in dst register

### 4. jit_module.rs

- Register rt_get_global, rt_set_global symbols (already have rt_call_native registered? check)

### 5. machine.rs + parity helpers

- Add `has_global_ops` to JitEntry
- needs_vm_ptr = has_string_ops || has_collection_ops || has_global_ops
- string_refs needed when needs_vm_ptr (already the case)
- Update all JitEntry construction sites

### 6. Tests

- type_analysis: GetGlobal produces Unknown, SetGlobal tracked, has_global_ops set
- JIT test: function that reads a global and returns it (rejected: Unknown return)
- JIT test: function that reads a global int, does arithmetic, returns int (works)
- JIT test: function that calls a global function via bridge
- Parity test fixture: global function call

## Not in scope

- Closure/GetUpvalue/SetUpvalue (deferred)
- Direct JIT-to-JIT calls (bridge handles transparently)
- Error propagation from bridge calls (returns null on error)
- Tiered compilation (Phase 2.5)

## Rollback

Revert branch. No schema/data changes.
