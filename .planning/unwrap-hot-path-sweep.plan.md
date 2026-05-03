# Unwrap Hot Path Sweep Plan

## Goal

Close production-readiness H1 by removing remaining production `unwrap()` usage in the scoped hot paths that can run while executing user programs.

## Findings

- `src/interpreter/mod.rs` has one `expect("BUG: channel mutex poisoned")`, already compliant with CLAUDE.md rule #6 because it documents an internal invariant.
- `src/vm/machine.rs` has two `expect("BUG: ...")` calls, already compliant internal invariants.
- `src/vm/machine.rs::run_until` still has `cached_closure.as_ref().unwrap()` in the VM execution loop.
- `src/stdlib/*.rs` hits are in test modules, not production paths.

## Implementation

1. Replace `cached_closure.as_ref().unwrap()` in `VM::run_until` with `expect("BUG: cached_closure is None after need_fetch guard always fills it")`.
   - This is a structural invariant inside the VM hot loop, not a recoverable user error.
   - Avoid adding a dead `VMError` branch to the hottest execution path.
2. Do not churn test `unwrap()` calls.
3. Do not replace existing `expect("BUG: ...")` calls that already explain impossible internal invariants.

## Tests

- `cargo fmt`
- `cargo test vm --lib`
- `cargo test`

## Rollback

Restore the previous cached-closure unwrap. No serialization or runtime API changes involved.
