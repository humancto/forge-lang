# Plan: 11B.1 — JIT String Operations

## Goal

Compile string concat, length, and comparison to native code via runtime bridge calls.

## Current State

- JIT only handles numeric functions (i64/f64)
- `type_analysis.rs` rejects functions with Concat/Len opcodes as `has_unsupported_ops`
- `runtime.rs` has bridge infrastructure (NaN-boxing encode/decode) but it's not wired up
- No mechanism to call runtime functions from JIT-compiled code

## Complexity Assessment

This requires fundamental JIT architecture changes:

1. Mixed-type register representation (I64 for both ints and GcRefs)
2. VM pointer passing to JIT functions (extra parameter)
3. Cranelift `call_indirect` or imported function refs for runtime bridges
4. Type tag checks in generated code

## Design: Minimal Viable Approach

### Phase 1: Add VM pointer parameter and runtime call mechanism

1. Add `vm_ptr: *mut VM` as first parameter to all JIT-compiled functions
2. Import runtime bridge function signatures into the Cranelift module
3. Generate `call` instructions to bridges

### Phase 2: Support Concat opcode

When encountering `OpCode::Concat`:

1. Both operands are in registers as tagged u64 (NaN-boxed)
2. Emit call to `rt_string_concat(vm_ptr, a_encoded, b_encoded) -> u64`
3. Store result in destination register

### Phase 3: Support string Len

`rt_string_len(vm_ptr, encoded) -> i64` — returns string length

### Phase 4: Support string Eq/NotEq

`rt_string_eq(vm_ptr, a_encoded, b_encoded) -> i64` (0 or 1)

### New runtime bridge functions needed

```rust
pub extern "C" fn rt_string_concat(vm_ptr: *mut VM, a: u64, b: u64) -> u64
pub extern "C" fn rt_string_len(vm_ptr: *mut VM, s: u64) -> i64
pub extern "C" fn rt_string_eq(vm_ptr: *mut VM, a: u64, b: u64) -> i64
```

### Value representation change

Switch from raw i64/f64 to NaN-boxed u64 for all values in JIT code. This is the biggest change:

- All registers become I64 (holding tagged u64)
- Arithmetic ops need to decode tags, perform operation, re-encode
- OR: keep the current approach but add a separate "mixed mode" for functions with strings

### Realistic scope

Full NaN-boxing JIT is a week-long project. For this roadmap item, a pragmatic approach:

1. Add runtime bridge function declarations to the JIT module
2. Add string runtime bridges to `runtime.rs`
3. In type_analysis, allow Concat/Len when all other ops are numeric
4. For Concat, emit bridge call; for Len on string result, emit bridge call
5. Keep the I64 representation — GcRef indices fit in I64

### Files to touch

1. `src/vm/jit/runtime.rs` — add string bridge functions
2. `src/vm/jit/ir_builder.rs` — import bridges, emit calls for Concat/Len
3. `src/vm/jit/type_analysis.rs` — relax unsupported ops check
4. `src/vm/jit/jit_module.rs` — pass VM pointer through
5. `src/vm/machine.rs` — update JIT dispatch to pass VM pointer

## Test strategy

- Existing JIT tests still pass
- New test: function with string concat JIT-compiled correctly
- New test: string len via JIT

## Rollback

Revert all JIT-related files.
