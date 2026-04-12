# 5A.1 — Replace `transmute(op)` with safe `TryFrom<u8>`

## Problem

`unsafe { std::mem::transmute(op) }` on an unchecked `u8` produces UB if `op` exceeds the OpCode variant count. Exists in 3 locations.

## Approach

1. Add `impl TryFrom<u8> for OpCode` in `bytecode.rs` using a match on all 63 variants (LoadConst=0 through Freeze=62)
2. Add compile-time assertion: `const _: () = assert!(OpCode::Freeze as u8 + 1 == 63);` to catch new variants missing from TryFrom
3. Replace all 3 transmute sites with `OpCode::try_from(op).map_err(...)`
4. Invalid opcodes produce a clean `VMError` instead of UB

## Error handling per site

- `machine.rs:1061` — `OpCode::try_from(op).map_err(|_| VMError::runtime(format!("invalid opcode: {op}")))` then `?` propagates to dispatch loop's error path
- `ir_builder.rs:88` — `OpCode::try_from(op).unwrap_or(return)` — skip unknown opcodes (JIT only compiles known arithmetic subset)
- `type_analysis.rs:60` — `OpCode::try_from(op).unwrap_or(return types)` — return early with current type info on unknown opcode

## Files

- `src/vm/bytecode.rs` — add TryFrom impl + compile-time assertion
- `src/vm/machine.rs:1061` — replace transmute
- `src/vm/jit/ir_builder.rs:88` — replace transmute
- `src/vm/jit/type_analysis.rs:60` — replace transmute

## Performance note

With `#[repr(u8)]` and contiguous discriminants 0..=62, rustc/LLVM should optimize the TryFrom match to a single bounds check + cast. No regression expected in the hot dispatch loop.

## Test strategy

- Existing 948 tests validate no regression
- Add a test that constructs a Chunk with an invalid opcode byte and verifies clean error

## Rollback

Revert the single commit.
