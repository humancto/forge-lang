# M9.8 — Structured Concurrency with `squad` Blocks

## TL;DR

Add `squad { spawn { ... } spawn { ... } }` blocks that guarantee:

1. **Automatic join** — all spawned tasks complete before the block exits.
2. **Cooperative cancellation** — if any task errors, remaining tasks are
   signalled via a shared `CancellationToken`. Cancellation is best-effort:
   it fires at Forge-level safe points (loops, function calls, statement
   boundaries). **Blocking builtins (HTTP, DB, FS) are NOT cancellable** —
   they run to completion. This is an inherent limitation of OS threads.
3. **Error propagation** — the first error is surfaced to the caller after
   all tasks have either completed or acknowledged cancellation.

## Design

### Syntax

```forge
let results = squad {
    spawn { fetch("https://api1.example.com") }
    spawn { fetch("https://api2.example.com") }
    spawn { fetch("https://api3.example.com") }
}
// results is an array: [response1, response2, response3]
// If any task failed, the squad block propagates the first error
// after joining all remaining tasks.
```

`squad` is a block expression that evaluates to an array of spawn
results in spawn order. Non-spawn statements inside the body execute
sequentially for side effects (setup, channel wiring, logging); their
values are NOT collected into the result array.

### Cancellation Token

```rust
// src/cancellation.rs (new file)
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self { Self(Arc::new(AtomicBool::new(false))) }
    pub fn cancel(&self) { self.0.store(true, Ordering::Release); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::Acquire) }
}
```

- Created per squad block.
- Passed to every child task spawned within the block via `Arc` sharing
  (not clone-by-value — child sees parent's cancel signal).
- When any task errors, the joining thread sets cancel.
- Tasks check the token cooperatively at:
  - **VM**: `OpCode::Loop` (backward jumps), `OpCode::Call` / `OpCode::CallMethod`
  - **Interpreter**: top of `exec_stmt()`, before each statement
- When a task sees cancellation, it returns
  `RuntimeError("task cancelled")` / `VMError("task cancelled")`.

**Cancellation scope — what IS and ISN'T cancellable:**

| Code pattern                | Cancellable?           | Why                                                                      |
| --------------------------- | ---------------------- | ------------------------------------------------------------------------ |
| `for x in items { ... }`    | Yes                    | backward jump checked                                                    |
| `repeat 1000 times { ... }` | Yes                    | backward jump checked                                                    |
| `some_fn()`                 | Yes                    | checked at call entry                                                    |
| `let x = 1 + 2 + 3`         | Yes (interp) / No (VM) | interp checks per-stmt; VM has no safe point in straight-line arithmetic |
| `fetch("url")`              | **No**                 | OS thread blocked in reqwest                                             |
| `db.query("...")`           | **No**                 | OS thread blocked in rusqlite                                            |
| `fs.read("file")`           | **No**                 | OS thread blocked in syscall                                             |
| `wait 10 seconds`           | **No**                 | OS thread sleeping                                                       |

This is documented prominently in the example file and CLAUDE.md.

### Semantics

1. Squad body executes sequentially on the current thread. Every
   `spawn { }` expression encountered at **any depth** within the body
   (including inside loops, conditionals, and called functions) is
   collected into the squad's handle list and runs concurrently.
2. After the body finishes executing, squad joins ALL collected handles.
3. If **all succeed**: returns `[result1, result2, ...]` in spawn order.
4. If **any fail**: sets cancellation token, joins remaining handles
   (blocking until they complete — no hard timeout), then propagates the
   first error as RuntimeError/VMError. Remaining tasks' results are
   discarded.
5. **No hard timeout.** We cannot kill OS threads. If a task is stuck in
   a blocking builtin, squad waits. This is honest. Users who need
   timeouts should use `await_timeout` on individual handles.

### Nested squads

- Inner squad creates its own token. Outer cancellation does NOT
  automatically set the inner token.
- However, the sequential code in the inner squad's body runs on the
  outer task's thread, which checks the outer token at statement
  boundaries. So if the outer squad cancels while the inner squad's
  body is still executing setup code, the inner squad's body will be
  interrupted. Once inner squad's tasks are spawned, they only see the
  inner token.
- This is consistent: each squad manages its own children, period.

### Early return / break / continue inside squad

- **`return` inside squad body**: triggers squad teardown — sets cancel
  token, joins all already-spawned handles, then returns. The function
  exits with the returned value, not the squad's results array.
- **`break` / `continue`**: apply to an enclosing loop, not the squad.
  If they cross the squad boundary (squad is inside a loop, break targets
  the loop), the squad must still join its spawned tasks before the break
  takes effect. Implementation: squad teardown runs in a Drop-like
  cleanup path.

### What does NOT change

- `spawn { }` outside a squad block works exactly as today (fire-and-forget).
- Channels, select, await_all, await_timeout — unchanged.
- No green threads. Still OS threads.

## Prerequisite: VM Spawn Error Propagation Fix

The VM's `spawn_thread` at `src/vm/machine.rs:59-62` currently does
`eprintln!` on error and stores `SharedValue::Null`. Squad requires
distinguishing "task returned null" from "task errored."

**Fix:** Add `SharedValue::Error(String)` to the `SharedValue` enum.

```rust
// In src/vm/machine.rs SharedValue enum:
pub enum SharedValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<SharedValue>),
    Map(Vec<(SharedValue, SharedValue)>),
    Object(IndexMap<String, SharedValue>),
    Error(String),  // NEW — task error message
}
```

Update `spawn_thread`:

```rust
// Instead of:
//   eprintln!("spawn error: {}", e.message);
//   *guard = Some(SharedValue::Null);
// Do:
*guard = Some(SharedValue::Error(e.message.clone()));
```

Update `OpCode::Await` / `await_all` VM join logic:

```rust
// When deserializing SharedValue back to Value:
SharedValue::Error(msg) => return Err(VMError::new(&msg)),
```

This fix also benefits existing `await_all` / `await_timeout` on the VM
(they'll now surface errors instead of returning null). It's a net
positive change outside of squad.

## Implementation Plan

### Phase 1: Prerequisite — VM error propagation fix

**Files**: `src/vm/machine.rs`

1. Add `SharedValue::Error(String)`.
2. Update `spawn_thread` error path.
3. Update `shared_to_value` / deserialization to surface errors.
4. Add test: `vm_spawn_error_surfaces_via_await`.

### Phase 2: Cancellation Token Infrastructure

**Files**: `src/cancellation.rs` (new), `src/lib.rs`, `src/interpreter/mod.rs`, `src/vm/machine.rs`

1. Create `src/cancellation.rs` with `CancellationToken`.
2. Add `cancellation_token: Option<CancellationToken>` to `Interpreter` and `VM`.
3. Add cancellation checks at safe points:
   - Interpreter: top of `exec_stmt()`.
   - VM: `OpCode::Loop`, `OpCode::Call`, `OpCode::CallMethod`.
4. Propagate token through `fork_for_spawn` (VM) and `spawn_task` (interpreter):
   - **VM**: `fork_for_spawn` already creates `VM::new()` and copies fields.
     Add: `child.cancellation_token = self.cancellation_token.clone()`.
   - **Interpreter**: `spawn_task` creates `Interpreter::new()` then copies
     `env`. Add: explicitly set `child.cancellation_token = self.cancellation_token.clone()`
     **after** construction, before executing body.

### Phase 3: Parser + AST

**Files**: `src/lexer/token.rs`, `src/lexer/mod.rs`, `src/parser/mod.rs`, `src/parser/ast.rs`

1. Add `Token::Squad` keyword to lexer.
2. Add `Stmt::Squad { body: Vec<Spanned<Stmt>> }` to AST.
   Also `Expr::Squad(Vec<Spanned<Stmt>>)` for expression position.
3. Parse rule: `squad { <stmts> }`.

### Phase 4: Interpreter Squad Implementation

**Files**: `src/interpreter/mod.rs`

1. Handle `Stmt::Squad` / `Expr::Squad`:
   - Save previous token: `let prev_token = self.cancellation_token.take()`.
   - Create fresh token: `let token = CancellationToken::new()`.
   - Set: `self.cancellation_token = Some(token.clone())`.
   - Track squad state: `self.squad_handles: Vec<Value::TaskHandle>` (new field,
     or use a local vec passed through a Cell/thread-local).
   - Execute body statements. Each `Stmt::Spawn` / `Expr::Spawn` encountered
     pushes its handle onto the squad handle list.
   - After body: join all handles in order.
   - If any returned `ResultErr`: call `token.cancel()`, join remaining,
     propagate first error.
   - Restore: `self.cancellation_token = prev_token`.
   - Return `Value::Array(results)`.

2. Handle early return / break / continue:
   - When `Signal::Return` / `Signal::Break` / `Signal::Continue` is caught
     during body execution: set cancel, join all spawned handles, then
     propagate the signal.

### Phase 5: VM/Compiler Squad Implementation

**Files**: `src/vm/compiler.rs`, `src/vm/machine.rs`, `src/vm/bytecode.rs`

1. Add `OpCode::SquadBegin` and `OpCode::SquadEnd`.
2. Compiler emits: `SquadBegin` → body instructions → `SquadEnd`.
3. VM execution:
   - `SquadBegin`: push new squad frame onto a `squad_stack: Vec<SquadFrame>`
     field on VM. `SquadFrame { token: CancellationToken, handles: Vec<Value> }`.
     Set `self.cancellation_token = Some(frame.token.clone())`.
   - During body: `OpCode::Spawn` checks if `squad_stack` is non-empty;
     if so, pushes handle onto top frame's handles vec (in addition to
     putting it on the stack for the user's code).
   - `SquadEnd`: pop frame, join all handles, handle errors, push result
     array onto stack. Restore previous token from outer frame (or None).

### Phase 6: Tests

**Interpreter tests** (~18):

1. Squad with two spawns, both succeed → array of results
2. Squad with three spawns, middle fails → error propagated, others cancelled
3. Squad with no spawns → empty array
4. Squad with one spawn → single-element array
5. Squad with let bindings + spawns → sequential code runs, spawns concurrent
6. Squad with channel communication between tasks
7. Nested squad blocks — inner completes, outer collects
8. Squad cancellation: looping task sees cancel at loop head
9. Squad with spawn that captures upvalues
10. Squad expression used in let binding
11. Squad with all tasks failing → first error propagated
12. Empty squad body → empty array
13. **Spawn inside loop inside squad** → all iterations' tasks join at squad end
14. **Spawn inside conditional inside squad** → only taken-branch spawns collected
15. **Early return inside squad** → spawned tasks cancelled, function returns
16. Squad result ordering matches spawn order, not completion order
17. Squad with non-spawn expressions → only spawn results in array
18. Squad cancellation does NOT kill blocking `wait` (pin: cooperative only)

**VM tests** (~18): Mirror of interpreter tests.

**Parity fixtures** (~6):

1. squad_basic.fg — two tasks, both succeed
2. squad_error_cancels.fg — one fails, looping other sees cancel
3. squad_with_channels.fg — inter-task communication
4. squad_nested.fg — inner + outer squads
5. squad_captures_upvalues.fg — closure capture across squad boundary
6. squad_expression.fg — squad as expression in let binding

### Phase 7: Documentation

- `examples/squad.fg` — idiomatic usage with cancellation scope table
- CHANGELOG.md entry
- CLAUDE.md Core Builtins update

## Risks

1. **Cooperative cancellation is best-effort.** Blocking builtins (HTTP,
   DB, FS, `wait`) are NOT cancellable. Documented prominently. Users
   who need hard timeouts should use `await_timeout` on individual handles.

2. **Thread wait on join.** Squad waits for ALL tasks to complete, even
   if some are stuck in blocking builtins. No hard timeout. This is honest
   — we cannot kill OS threads in Rust. A future green-thread scheduler
   (`src/vm/green.rs` stub exists) could change this.

3. **VM error propagation (Phase 1 prerequisite).** Without fixing
   `SharedValue` error representation, squad cannot detect task failures.
   Phase 1 addresses this first.

4. **Spawn-in-loop fan-out.** `for i in range(10000) { spawn { ... } }`
   inside a squad creates 10,000 OS threads. No guard rail. This matches
   existing `spawn` behavior — squad doesn't make it worse, but it does
   make the join mandatory (so you WILL wait for all 10K). Users should
   use channels + a fixed worker pool for high fan-out. Document this.

5. **`fork_for_spawn` drops closures.** Known M9.5 finding. Squad tasks
   can't construct ADTs inside spawned blocks. Orthogonal.

## Files Touched

- `src/cancellation.rs` — NEW, ~15 lines
- `src/lib.rs` — register module
- `src/lexer/token.rs` — add `Squad` keyword
- `src/lexer/mod.rs` — map "squad" → Token::Squad
- `src/parser/ast.rs` — add `Stmt::Squad`, `Expr::Squad`
- `src/parser/mod.rs` — parse `squad { }` blocks
- `src/interpreter/mod.rs` — squad execution, cancellation checks, early-return handling
- `src/vm/machine.rs` — SharedValue::Error, squad execution, cancellation checks
- `src/vm/compiler.rs` — compile squad blocks
- `src/vm/bytecode.rs` — SquadBegin/SquadEnd opcodes
- `src/interpreter/tests.rs` — ~18 tests
- `src/vm/squad_tests.rs` — NEW, ~18 tests
- `tests/parity/supported/squad_*.fg` — 6 fixtures
- `examples/squad.fg` — NEW
- `CHANGELOG.md` — entry
- `CLAUDE.md` — Core Builtins update

## Test Plan

- `cargo test --lib` → all existing tests pass + ~36 new squad tests.
- `cargo test --lib parity_` → all existing + 6 new fixtures.
- `./target/debug/forge run examples/squad.fg` → expected output.
- `./target/debug/forge --interp run examples/squad.fg` → same.
- Manual: squad with a looping task + a failing task — looping task
  sees cancel within a few ms. Blocking-builtin task does NOT see cancel
  (documented limitation).

## Phases (execution order)

1. Prerequisite: VM spawn error propagation fix (SharedValue::Error).
2. Cancellation token infrastructure.
3. Parser + AST changes.
4. Interpreter squad implementation.
5. VM/compiler squad implementation.
6. Tests (interpreter + VM + parity).
7. Documentation (example + CHANGELOG + CLAUDE.md).
8. rust-expert review → revise → PR → final review → merge.
