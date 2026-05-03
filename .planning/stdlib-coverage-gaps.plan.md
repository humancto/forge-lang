# Stdlib Coverage Gaps Plan

## Goal

Close production-readiness G1 by filling deterministic unit-test gaps in `crypto`, `regex`, `json`, and `time`.

## Current State

- `crypto.rs` already has broad happy-path vectors and basic error tests.
- `regex_module.rs` already covers each public regex function plus invalid patterns and wrong args.
- `json_module.rs` already covers parse/stringify/pretty/valid/merge and round-trips.
- `time.rs` has strong coverage for parsing, formatting, arithmetic, zones, and calendar helpers, but its module-list test omits some exported functions and several deterministic public functions still lack direct tests.

## Implementation

1. Add missing `time` module export assertions for all functions in `create_module`.
2. Add deterministic `time` tests for:
   - `time.zone` converting a fixed timestamp to a named timezone
   - `time.elapsed` / `time.measure` returning positive epoch milliseconds without asserting wall-clock precision
   - `time.sleep(0)` returning quickly without timing flakes
   - `time.local` returning an object tagged `Local`
   - `time.is_weekend`, `time.is_weekday`, and `time.day_of_week`
   - wrong-argument errors for representative time helpers
3. Add a few small edge-case tests outside `time` only where they add signal:
   - JSON rejects non-string map keys during stringify/pretty
   - regex invalid pattern errors propagate from another public function, not just `test`
   - crypto hash functions reject missing/wrong arguments consistently

## Tests

- `cargo fmt`
- `cargo test stdlib::crypto --lib`
- `cargo test stdlib::regex_module --lib`
- `cargo test stdlib::json_module --lib`
- `cargo test stdlib::time --lib`
- `cargo test`

## Rollback

Remove the added unit tests and this plan file. No runtime behavior changes expected.
