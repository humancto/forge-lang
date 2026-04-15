# 7B.2 — Audit and remove production panic! calls

## Audit Results

Only **10** panic-family macros in production code (not 92 as initially estimated — the original count included test code).

## Action Plan

### Convert to proper errors (3 items, user-reachable):

1. **compiler.rs:99** — `panic!("register overflow")` → return `Err(...)` from `alloc_reg()`, propagate through compiler
2. **machine.rs:160** — `panic!("JIT dispatch > 8 args i64")` → return runtime error
3. **machine.rs:213** — `panic!("JIT dispatch > 8 args f64")` → return runtime error

### Leave as-is (5 items, defensive unreachable):

- parser.rs:305, 1196 — exhaustive match guards, structurally unreachable
- compiler.rs:1634 — And/Or handled before this point
- ir_builder.rs:231, 244 — all comparison opcodes explicitly handled

### Testing infrastructure (4 items):

- testing/parity.rs:43, 55, 121, 138 — test harness, panics are appropriate here

## Edge Cases

- Compiler error propagation: `alloc_reg()` is called frequently; need to ensure `?` propagation doesn't break the compiler flow
- JIT dispatch: graceful fallback to interpreter for 9+ arg functions

## Test Strategy

- Write a test with 255+ locals to verify compiler error message
- Existing JIT tests cover < 8 args; add a test confirming 9+ args don't crash
