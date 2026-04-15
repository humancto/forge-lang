# Plan: Auto-JIT for any function type (revised after expert review)

## Goal

Remove restrictions that prevent JIT compilation of functions mixing floats with strings/collections. Expand auto-JIT coverage beyond pure-int or pure-float functions.

## Expert Review Findings (addressed)

- **S1/S3: Per-register Cranelift types with mixed ABI is too complex.** Per-register F64/I64 types create ABI mismatches (dispatch can't marshal args correctly) and Move/GetLocal/SetLocal need type coercions.
- **S2: Type analysis is single-pass, can't handle loop-carried register conflicts.**
- **Fix: Use I64 for everything in the Cranelift signature. Bitcast F64↔I64 internally.** This avoids all ABI/dispatch changes while supporting float+ops mixing.

## Revised Approach: I64-everywhere with internal bitcast

### Key Insight

Keep the Cranelift function signature as `(I64, I64, ...) -> I64` always. Inside the function body:

- Float values are stored as their IEEE 754 bit representation in I64 variables
- Before float arithmetic, `bitcast I64 -> F64`, compute, then `bitcast F64 -> I64` to store result
- Int values pass through as-is
- Bridge calls already use I64 (tagged), so no changes needed there

This approach:

- Eliminates ABI mismatch (all params/returns are I64)
- Eliminates per-register type issues (all Cranelift variables are I64)
- Eliminates Move/GetLocal type conflicts
- Eliminates branch-dependent type issues
- Keeps dispatch unchanged for int/string/collection functions
- Only adds bitcast overhead around float operations

### Changes

**src/vm/jit/type_analysis.rs:**

- Remove the float+string/collection rejection (line 298-300)
- The `has_float` flag stays but now means "function uses float ops" not "function uses all-F64 mode"

**src/vm/jit/ir_builder.rs:**

- Remove the `use_float` toggle that switches everything to F64
- All variables declared as I64
- All params/returns as I64
- For float operations (when both/either operand is Float type):
  - `bitcast I64 -> F64` for operands
  - Perform float arithmetic (fadd, fsub, fmul, fdiv)
  - `bitcast F64 -> I64` for result
- For LoadConst with Float constant: `f64const` then `bitcast F64 -> I64`
- For comparisons involving float: bitcast, fcmp, then convert bool result to I64
- For JumpIfFalse/JumpIfTrue on float register: bitcast, fcmp with 0.0

**src/vm/machine.rs (dispatch):**

- Remove the `uses_float` dispatch path entirely
- All JIT dispatch uses the i64 path
- Float args: convert with `f64::to_bits() as i64`
- Float returns: when `uses_float && !returns_obj`, decode with `f64::from_bits(result as u64)`
- This eliminates jit_call_f64 entirely

**src/vm/jit_tests.rs:**

- Add test: function mixing float arithmetic and string ops
- Add test: function with float args that also uses global access
- Update existing float tests (now use I64 bitcast path)

## Edge Cases

- Float→int bitcast preserves exact bits (IEEE 754)
- NaN values: bitcast preserves NaN bits, Cranelift fcmp handles NaN per IEEE 754
- Negative zero: bitcast preserves sign bit
- Large float values: f64::to_bits() always works, no overflow
- Self-recursive calls: I64 uniform ABI means self-calls just work
- Bool registers: always I64 (0 or 1), no change needed

## Risk Mitigation

- Bitcast overhead: 0 cycles on modern CPUs (register reinterpretation, no instruction emitted)
- All existing float tests must still pass with the new bitcast path
- The dispatch simplification (removing jit_call_f64) reduces code and eliminates a branch

## Test Strategy

- All existing 1198 tests must pass
- New tests for mixed float+string/collection functions
- Run examples (hello.fg, functional.fg)
- Verify existing float tests (jit_float_arithmetic, jit_float_negation, etc.) still produce correct results

## Rollback

Revert the bitcast changes — restore the dual F64/I64 mode.
