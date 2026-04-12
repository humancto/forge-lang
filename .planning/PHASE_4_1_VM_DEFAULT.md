# Phase 4.1 — VM as Default Engine

Written: 2026-04-11

---

## Goal

Make the VM the default execution engine for `forge run`. Currently the interpreter is default and the VM is opt-in via `--vm`. This phase implements the three missing expression types (`must`, `ask`, `freeze`) so the VM can handle all programs the interpreter can (except decorator-driven HTTP servers, which remain interpreter-only).

After this change:

- `forge run file.fg` → uses VM (was interpreter)
- `forge run --interp file.fg` → uses interpreter (new flag for fallback)
- Programs using `@server`/`@get` decorators → auto-fallback to interpreter with info message

## Design

### 1. New Opcodes: Must, Ask, Freeze

**Must** (`must expr`): Evaluate inner expression. If result is `Err(msg)` or `null`, crash with error. If `Ok(val)`, unwrap to `val`. Otherwise pass through.

```
OpCode::Must  A=dst, B=src
```

Machine logic:

- Check `registers[B]` — if `ObjKind::ResultErr(e)`, return `VMError("must failed: {e}")`
- If `Value::Null`, return `VMError("must failed: got null")`
- If `ObjKind::ResultOk(v)`, store unwrapped value in `registers[A]`
- Otherwise, copy value to `registers[A]`

**Ask** (`ask "prompt"`): Call LLM API synchronously. Returns string response or null.

```
OpCode::Ask  A=dst, B=prompt_reg
```

Machine logic:

- Read prompt string from `registers[B]`
- Read env vars: `FORGE_AI_KEY`/`OPENAI_API_KEY`, `FORGE_AI_MODEL`, `FORGE_AI_URL`
- Call `crate::runtime::client::fetch_blocking()` with OpenAI-compatible API
- Parse JSON response, extract `choices[0].message.content`
- Store result string in `registers[A]`

**Freeze** (`freeze expr`): Wrap value to prevent mutation.

```
OpCode::Freeze  A=dst, B=src
```

Machine logic:

- Add `ObjKind::Frozen(Value)` variant to `ObjKind` enum
- Wrap `registers[B]` in a Frozen object, store GcRef in `registers[A]`
- SetField/SetIndex must check for Frozen and return error

### 2. Compiler Changes

In `src/vm/compiler.rs`, replace the silent pass-through:

```rust
// Before:
Expr::Must(inner) | Expr::Freeze(inner) | Expr::Ask(inner) => {
    compile_expr(c, inner, dst)?;
}

// After:
Expr::Must(inner) => {
    let src = c.alloc_reg();
    compile_expr(c, inner, src)?;
    c.emit(encode_abc(OpCode::Must, dst, src, 0), c.current_line);
    c.free_to(src);
}
Expr::Ask(inner) => {
    let src = c.alloc_reg();
    compile_expr(c, inner, src)?;
    c.emit(encode_abc(OpCode::Ask, dst, src, 0), c.current_line);
    c.free_to(src);
}
Expr::Freeze(inner) => {
    let src = c.alloc_reg();
    compile_expr(c, inner, src)?;
    c.emit(encode_abc(OpCode::Freeze, dst, src, 0), c.current_line);
    c.free_to(src);
}
```

### 3. Remove VM Rejection

In `src/main.rs`, remove `must expressions`, `ask expressions`, `freeze expressions` from `collect_vm_incompatible_expr()`. Keep `@server`/decorator rejection.

### 4. Default Engine Flip

In `src/main.rs`, change `run_source()`:

- Default to VM instead of interpreter
- Add `--interp` flag to explicitly use interpreter
- Keep `--vm` flag (now a no-op but accepted for backwards compat)
- Auto-fallback to interpreter when decorators are detected, with info message

### 5. Parity Tests

Move `must`, `ask`, `freeze` from `tests/parity/unsupported_vm/` to `tests/parity/` so they're tested in parity mode.

Add new parity tests:

- `must_unwraps_ok.fg` — `must Ok(42)` returns 42
- `must_crashes_on_err.fg` — `must Err("fail")` crashes
- `freeze_prevents_mutation.fg` — frozen object rejects SetField

---

## File Change Summary

| File                 | Change                                                 |
| -------------------- | ------------------------------------------------------ |
| `src/vm/bytecode.rs` | Add `Must`, `Ask`, `Freeze` opcodes                    |
| `src/vm/compiler.rs` | Emit new opcodes instead of pass-through               |
| `src/vm/machine.rs`  | Handle Must/Ask/Freeze in dispatch loop                |
| `src/vm/value.rs`    | Add `ObjKind::Frozen(Value)` variant                   |
| `src/main.rs`        | Flip default engine, add `--interp`, remove rejections |
| `src/vm/mod.rs`      | Add tests for must/ask/freeze                          |
| `tests/parity/`      | Move must/freeze from unsupported_vm/                  |

---

## Test Plan

1. `must_unwraps_ok` — `must Ok(42)` → 42
2. `must_crashes_on_err` — `must Err("x")` → runtime error
3. `must_crashes_on_null` — `must null` → runtime error
4. `must_passes_through_non_result` — `must 42` → 42
5. `freeze_wraps_value` — `freeze obj` creates frozen wrapper
6. `freeze_rejects_set_field` — setting field on frozen → error
7. `ask_without_api_key` — returns error when no key set
8. `vm_is_default` — running without flags uses VM
9. `interp_flag_works` — `--interp` uses interpreter
10. `decorator_auto_fallback` — `@server` programs fall back to interpreter
11. All existing 925 tests still pass
12. All examples pass under default (VM) engine

---

## Risks

1. **ask requires network** — tests must mock or skip. Use env var check.
2. **Frozen mutation check** — must intercept SetField/SetIndex, slight perf cost.
3. **Backwards compat** — `--vm` flag must still work (no-op).
4. **Decorator fallback** — must detect decorators before compilation, not during.
