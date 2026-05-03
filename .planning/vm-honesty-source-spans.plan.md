# VM Honesty + Source Spans Plan

## Goal

Finish the still-relevant parts of `PRODUCTION_READINESS.md` Group E:

- Stop silently accepting VM-only-unsupported standalone decorators when the compiler is called directly.
- Extend VM runtime error traces from line-only frames to line + column frames.

## Current State

Some E1 bullets are stale:

- `schedule`, `watch`, `spawn`, `ask`, `await`, `must`, and `freeze` now have VM opcodes and runtime implementations.
- The CLI already preflights decorator-driven runtime features and rejects `@server` / route decorators before VM execution.
- The compiler still has `Stmt::DecoratorStmt(_) => Ok(())`, which is unsafe if tests or embedders call `vm::compiler::compile` directly.

E2 is partially done:

- `Chunk.lines` records per-op source lines.
- `VMError` collects stack frames and `Display` renders `at <fn> (line N)`.
- `SpannedStmt` already has `col`, but the compiler drops it before bytecode emission.

## Implementation

1. Add column metadata beside existing line metadata.
   - Add `cols: Vec<usize>` to `Chunk`.
   - Add `Chunk::emit_at(instruction, line, col)` and keep `Chunk::emit(instruction, line)` only as a compatibility wrapper for hand-built test chunks, where it pushes `col = 0`.
   - Add a debug invariant after emission that `code.len() == lines.len() == cols.len()`.
   - Update bytecode serialization because line tables are serialized today:
     - Bump `VERSION_MINOR` from `1` to `2`.
     - Write a column table immediately after the existing line table.
     - Pass root bytecode version into recursive `read_chunk_inner`.
     - For old `1.1` bytecode, synthesize `cols = vec![0; code.len()]`.
     - For `1.2+`, read the column table, cap it like the line table, and reject malformed chunks where `lines.len()` or `cols.len()` does not match `code.len()`.

2. Track the current source span in the compiler.
   - Add `current_col` next to `current_line`.
   - Add a small `set_current_span(line, col)` helper plus a `set_span(&SpannedStmt)` convenience helper.
   - Replace every `current_line = s.line` / `current_line = spanned.line` assignment with the helper. A mechanical grep for `current_line = .*\\.line` must come back empty.
   - Update every child-compiler initialization that copies `current_line = c.current_line` to also copy `current_col = c.current_col`.
   - Update `Compiler::emit` to call `Chunk::emit_at(inst, actual_line, actual_col)`, where `actual_col` falls back to `current_col` exactly as line falls back to `current_line`.
   - Cover `compile_spawn_body`, `compile()` / `compile_module()` / REPL compile loops, `FnDef`, `ScheduleBlock`, `WatchBlock`, statement `Spawn`, expression `Spawn`, lambda, block, match/when arms, try/catch, safe/timeout/retry, loops, and squad.

3. Render columns in VM stack traces.
   - Add `col: usize` to `StackFrame`.
   - Update `collect_stack_trace` to read `chunk.cols[ip - 1]` with the same bounds guard used for `chunk.lines`.
   - Render frames as `at <function> (line N, col M)` when `col > 0`; keep `line N` fallback for old/deserialized hand-built chunks with no column.

4. Make standalone decorators fail honestly in direct VM compilation.
   - Replace `Stmt::DecoratorStmt(_) => Ok(())` with a `CompileError` saying VM does not support standalone decorator-driven runtime features and to use the interpreter.
   - Do not reject decorators attached to `FnDef`: metadata decorators like `@test`, `@skip`, `@before`, and `@after` remain supported by existing compiler paths.

## Tests

- Add/adjust VM tests:
  - Existing VM error display test should assert the rendered trace includes `col`.
  - Add a compile test for standalone decorator rejection (`@server(...)` or `@unknown`) through `compiler::compile`, not only CLI preflight.
  - Add a compile test proving metadata decorators on functions (`@test fn ...`) still compile.
  - Add a nested function or lambda error test proving child compiler chunks preserve `col > 0`.
- Add/adjust serialization tests:
  - Round-trip a hand-built chunk and assert `cols` round trips.
  - Round-trip a compiled program and assert `cols` round trips.
  - Add a v1.1 compatibility fixture/byte stream with no column table and assert `cols` is synthesized as zeros.
- Run:
  - `cargo test vm_error_stack_trace --lib`
  - `cargo test serialize --lib`
  - `cargo test parity_corpus_ --lib`
  - `cargo test vm::compiler --lib` or the focused compiler test filter
  - `cargo test`

## Risks

- Bytecode serialization currently writes line tables. Adding columns without a version bump or backward-compatible decoder would corrupt `.fgc` compatibility.
- Many compiler call sites pass `0` for line; the fallback behavior must remain unchanged for line while adding equivalent column fallback.
