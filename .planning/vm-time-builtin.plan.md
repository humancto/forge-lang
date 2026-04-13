# Plan: VM time() builtin

## Goal

Port `time()` builtin from interpreter to VM. Returns a datetime object with iso, unix, year, month, day, hour, minute, second, weekday, timezone fields.

## Design

- Call `chrono::Utc::now()` directly in builtins.rs
- Build `ObjKind::Object(IndexMap)` with same fields as interpreter's `datetime_to_value()`
- String fields allocated via `gc.alloc(ObjKind::String(...))`
- Int fields stored as `Value::Int`

## Files

1. `src/vm/builtins.rs` — add "time" handler
2. `src/vm/machine.rs` — register "time" builtin
3. `src/vm/async_tests.rs` — test (or new test file section)

## Tests

1. `vm_time_returns_object` — time() returns object with expected keys
2. `vm_time_unix_positive` — unix field > 1_700_000_000

## Rollback

Revert builtins.rs, machine.rs, test file.
