# Phase 3.5 — `schedule` / `watch` in VM

Written: 2026-04-11 (v2 — revised after reviewer feedback)
Prerequisite: Phase 3.1 (async VM runtime) — DONE (PR #14)

---

## Goal

Make `schedule every N unit { body }` and `watch "path" { body }` work in `--vm` mode. Currently the compiler silently drops both (returns `Ok(())`). The interpreter implements these via background threads with `fork_for_background_runtime()` — we follow the same pattern using `fork_for_spawn` + `SendableVM` from Phase 3.1.

## Key Design Decisions

### Two new opcodes (`Schedule`, `Watch`)

Schedule and watch are **fire-and-forget background loops** — the body runs in a background thread indefinitely with no value returned to the parent VM. The semantics differ enough from Spawn to warrant dedicated opcodes.

1. **`OpCode::Schedule`** — `A=closure_reg, B=interval_reg, C=unit_reg`
   - All three operands are **register indices** (ABC encoding, 8 bits each)
   - The unit string is loaded into a register via `LoadConst` before this opcode
   - Evaluates interval from register B (must be Int, must be > 0; errors on non-positive)
   - Reads unit string from register C ("seconds", "minutes", "hours")
   - Computes sleep duration in seconds
   - Forks VM, transfers closure, spawns background thread with infinite loop

2. **`OpCode::Watch`** — `A=closure_reg, B=path_reg`
   - Path evaluated from register B (must be String; errors with "watch requires a string path" matching interpreter)
   - Forks VM, transfers closure, spawns background thread with mtime polling

### Compiler changes

The compiler currently has:

```rust
Stmt::ScheduleBlock { .. } => Ok(()),
Stmt::WatchBlock { .. } => Ok(()),
```

Replace with proper compilation:

**ScheduleBlock:**

1. Compile `interval` expr into a register
2. Load unit string into a register via `LoadConst` (not C-field — constant indices need 16 bits)
3. Compile body as a closure (sub-compiler, same pattern as Spawn)
4. Emit `OpCode::Schedule` with `A=closure_reg, B=interval_reg, C=unit_reg`

**WatchBlock:**

1. Compile `path` expr into a register
2. Compile body as a closure (sub-compiler)
3. Emit `OpCode::Watch` with `A=closure_reg, B=path_reg`

Both use the same sub-compiler pattern as `Stmt::Spawn` (lines ~1446-1475 in compiler.rs):

- Create sub-compiler with `parent_locals` / `parent_upvalues`
- Compile body statements
- Emit `ReturnNull` at end
- Push prototype chunk with `upvalue_sources` propagated (critical — fixed in 3.1 for spawn)
- Emit `Closure` opcode in parent
- Capture upvalues with `GetLocal`/`GetUpvalue`

### Machine changes

**GC safety for repeated closure calls:**
The closure `Value::Obj(GcRef)` must survive GC across repeated calls in the loop. After each `call_value` returns, the closure's GcRef could be collected since it's not rooted in any frame. Solution: store the closure in `registers[0]` of the child VM before entering the loop so the GC always sees it as a root.

**Schedule handler (`SendableVM::run_loop`):**

```rust
fn run_loop(mut self, closure: Value, interval: Duration) {
    let vm = &mut self.0;
    // Root the closure in register 0 so GC can't collect it
    vm.registers[0] = closure.clone();
    loop {
        std::thread::sleep(interval);
        let _ = vm.call_value(closure.clone(), vec![]);
        // Re-root after call (call_value may have moved registers)
        vm.registers[0] = closure.clone();
    }
}
```

Free function wrapper (same pattern as `spawn_thread`):

```rust
fn spawn_schedule_thread(sendable: SendableVM, closure: Value, interval: Duration) {
    std::thread::spawn(move || { sendable.run_loop(closure, interval); });
}
```

**Watch handler (`SendableVM::run_watch`):**

```rust
fn run_watch(mut self, closure: Value, path: String) {
    let vm = &mut self.0;
    vm.registers[0] = closure.clone();
    let mut last_modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let current = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if current != last_modified {
            last_modified = current;
            let _ = vm.call_value(closure.clone(), vec![]);
            vm.registers[0] = closure.clone();
        }
    }
}
```

Free function wrapper:

```rust
fn spawn_watch_thread(sendable: SendableVM, closure: Value, path: String) {
    std::thread::spawn(move || { sendable.run_watch(closure, path); });
}
```

**Runtime validation in opcode handler:**

- Schedule: if interval register is not `Value::Int` or value <= 0, return `VMError` matching interpreter fallback (default 60s for non-int, error for non-positive)
- Watch: if path register is not a string `ObjKind::String`, return `VMError("watch requires a string path")`

### VM incompatibility list update (main.rs)

Remove `"schedule blocks"` and `"watch blocks"` from `collect_vm_incompatible_stmt`. Update `--vm` doc string.

### Parity test update

Delete `tests/parity/unsupported_vm/schedule_block.fg` (no longer unsupported).
No `watch_block.fg` exists in the unsupported_vm directory — verified.

### Test plan

Add tests in `src/vm/mod.rs` under `mod schedule_watch_tests`.

**Testing strategy for background threads:**

- Schedule/watch run on child VMs via `fork_for_spawn` — the child's `self.output` is NOT the parent's
- The child VM's `println` writes to stdout AND `self.output`, but since the child is on another thread, we can't access `self.output` from the test
- For "fires" tests: write to a **temp file** from the schedule/watch body using `fs.write`, then check the file in the test after sleeping
- For "compiles" tests: just verify compilation succeeds (no runtime needed)
- For upvalue capture tests: verify compilation succeeds with upvalue references
- JIT is not used in child VMs (fork_for_spawn creates empty jit_cache) — no JIT interaction concerns

**Tests:**

1. **vm_schedule_compiles** — `schedule every 1 seconds { let x = 1 }` compiles without error
2. **vm_schedule_fires** — schedule with `every 1 seconds`, body writes to temp file, test sleeps 1.5s, checks file exists with content
3. **vm_schedule_minutes_multiplier** — unit test for interval computation: verify "minutes" multiplies by 60
4. **vm_schedule_hours_multiplier** — unit test for interval computation: verify "hours" multiplies by 3600
5. **vm_schedule_captures_variable** — `let x = 42; schedule every 1 seconds { let y = x }` compiles (verifies upvalue capture works)
6. **vm_watch_compiles** — `watch "somefile" { let x = 1 }` compiles without error
7. **vm_watch_fires_on_change** — create temp file, watch it, modify file after 0.5s in another thread, check body executed (wrote to second temp file)
8. **vm_watch_no_fire_without_change** — create temp file, watch it, wait 1.5s, verify output file does NOT exist
9. **vm_schedule_non_positive_interval** — `schedule every 0 seconds { }` or `schedule every -1 seconds { }` — verify error or default behavior

### Bytecode serialization

Add `Schedule` and `Watch` to `serialize.rs` round-trip test.

---

## File Change Summary

| File                                            | Change                                                                                                                                             |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/vm/bytecode.rs`                            | Add `Schedule` and `Watch` opcodes                                                                                                                 |
| `src/vm/compiler.rs`                            | Compile `ScheduleBlock` and `WatchBlock` stmts                                                                                                     |
| `src/vm/machine.rs`                             | Handle `Schedule` and `Watch` opcodes; add `run_loop`/`run_watch` on `SendableVM`; add `spawn_schedule_thread`/`spawn_watch_thread` free functions |
| `src/vm/mod.rs`                                 | Add 9 schedule/watch tests                                                                                                                         |
| `src/vm/serialize.rs`                           | Add new opcodes to round-trip test                                                                                                                 |
| `src/main.rs`                                   | Remove schedule/watch from VM incompatibility list                                                                                                 |
| `tests/parity/unsupported_vm/schedule_block.fg` | Delete                                                                                                                                             |
| `CHANGELOG.md`                                  | Add entry under [Unreleased]                                                                                                                       |

---

## Risks & Mitigations

1. **Thread cleanup on VM drop:** Schedule/watch threads run forever. When the parent VM exits, detached threads are killed by OS process exit. Same behaviour as the interpreter. No cleanup needed for now — a future improvement could use a `CancellationToken`.

2. **Upvalue capture in schedule/watch closures:** Same pattern as Spawn — already proven in 3.1. Must propagate `upvalue_sources` to the prototype chunk.

3. **Timing-sensitive tests:** Tests that verify "fires" use file I/O as the verification mechanism with generous sleep margins (1.5s for 1s intervals). Tests check for "at least one execution happened" (file exists with content) rather than exact counts.

4. **Watch polling frequency:** Fixed at 1 second (same as interpreter). Not configurable for now.

5. **Error handling in background body:** Errors are silently swallowed (same as interpreter: `let _ = ...`). The background thread continues running.

6. **GC safety in loop:** Closure rooted in `registers[0]` before each call and re-rooted after each call returns. This ensures the GC sees the closure as a live root during collection.

7. **Upvalue mutation is one-way:** `transfer_closure` copies upvalue values via `SharedValue` (deep copy). Mutations inside the background body do NOT propagate back to the parent. This matches interpreter semantics (`fork_for_background_runtime` also deep-copies).

---

## Audit Trail

### Review 1 (2026-04-11, forge-vm-reviewer)

**Verdict: REVISE**

| #   | Issue                                                           | Severity    | Resolution                                                                             |
| --- | --------------------------------------------------------------- | ----------- | -------------------------------------------------------------------------------------- |
| S1  | Schedule C-field can't hold const pool index (8-bit limit)      | Showstopper | Fixed: load unit string into register via LoadConst, use ABC with all register indices |
| S2  | Closure GcRef may be collected by GC across repeated loop calls | Showstopper | Fixed: root closure in registers[0] before/after each call                             |
| B1  | Test timing ambiguity for vm_schedule_fires                     | Bug         | Fixed: specified exact interval (1s) and wait (1.5s)                                   |
| B2  | No validation for non-positive interval                         | Bug         | Fixed: added error/default handling spec                                               |
| B3  | Watch path evaluation timing (compile-time vs runtime)          | Bug (minor) | Acknowledged: matches interpreter semantics                                            |
| R1  | Parent vm.output won't see child thread output                  | Risk        | Fixed: tests use temp file I/O instead of vm.output                                    |
| R2  | Thread leak in tests                                            | Risk        | Accepted: daemon threads die on process exit, same as interpreter                      |
| R3  | fork_for_spawn doesn't copy output vec                          | Risk        | By design: child has own output, println still writes to stdout                        |
| R4  | Upvalue mutation is one-way (deep copy)                         | Risk        | Documented: matches interpreter semantics                                              |
| M1  | No error for non-string watch path                              | Missing     | Fixed: added "watch requires a string path" error                                      |
| M2  | No error for non-integer schedule interval                      | Missing     | Fixed: added fallback/error spec                                                       |
| M3  | Watch parity test file                                          | Missing     | Verified: no watch_block.fg exists, nothing to delete                                  |
| M4  | No complex body test                                            | Missing     | Accepted: basic tests sufficient for M1 scope                                          |
| M5  | No multiple schedule/watch test                                 | Missing     | Accepted: same threading pattern as spawn, proven in 3.1                               |
| M6  | JIT interaction                                                 | Missing     | Documented: child VMs have empty jit_cache, no interaction                             |
| N1  | Dead pseudocode in plan                                         | Nit         | Fixed: removed                                                                         |
| N2  | Contradictory headings                                          | Nit         | Fixed: removed "no new opcodes" heading                                                |
| N3  | Consider ScheduleLoop/WatchLoop naming                          | Nit         | Declined: Schedule/Watch is clear enough, matches AST names                            |
| N4  | Duration vs u64 for sub-second intervals                        | Nit         | Accepted: use Duration in implementation, u64 seconds from Forge code                  |

---

## Implementation Order

1. Add opcodes to `bytecode.rs`
2. Add serialization support in `serialize.rs`
3. Implement compiler for both stmts in `compiler.rs`
4. Implement machine handlers + `run_loop`/`run_watch` in `machine.rs`
5. Update incompatibility list in `main.rs`
6. Delete parity test
7. Add tests in `mod.rs`
8. Update CHANGELOG
9. `cargo test` — all green
