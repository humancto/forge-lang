# Phase 4.5: Debugger (DAP)

## Approach

Implement a Debug Adapter Protocol server for VS Code step-through debugging. Follow the same stdio-based pattern as the LSP server.

## Architecture

- New `src/dap/mod.rs` module — DAP protocol handler
- Add `Command::Dap` to main.rs CLI
- Add debug hooks to the interpreter's `exec_stmts()` loop
- Use channels for communication between DAP server thread and interpreter thread

## Key Design Decisions

1. **Interpreter only** — DAP works with the tree-walking interpreter, not the VM. The interpreter already has `current_line`, scoped `Environment`, and cooperative `cancelled` flag.

2. **Stdio-based protocol** — Same Content-Length framing as LSP. No async needed.

3. **Thread model** — DAP server runs on main thread reading stdin. Interpreter runs on a spawned thread. Communication via `mpsc::channel` and `Arc<Mutex<DebugState>>`.

4. **DebugState** — Shared state between DAP server and interpreter:
   - `breakpoints: HashSet<usize>` — line numbers with breakpoints
   - `action: DebugAction` — Continue, StepOver, StepIn, StepOut, Pause
   - `paused: Condvar` — interpreter waits here when stopped

## DAP Messages to Implement

### Required (minimum viable debugger):

- `initialize` → capabilities response
- `launch` → start interpreter on a file
- `setBreakpoints` → store breakpoints per file
- `threads` → single thread (id=1)
- `stackTrace` → current function + line from interpreter call stack
- `scopes` → local scope
- `variables` → enumerate variables from Environment
- `continue` → resume execution
- `next` → step over (execute next statement)
- `stepIn` → step into function calls
- `stepOut` → step out of current function
- `disconnect` → clean shutdown
- `configurationDone` → acknowledge

### Events to emit:

- `initialized` — after init
- `stopped` — on breakpoint hit, step complete, or exception
- `terminated` — program finished
- `output` — capture println/say output

## Files to Change

1. **NEW `src/dap/mod.rs`** (~400 lines) — DAP server, message dispatch, debug state
2. **`src/interpreter/mod.rs`** — Add `debug_state` field, check breakpoints in `exec_stmts()`
3. **`src/main.rs`** — Add `mod dap`, `Command::Dap { file }`, wire up

## Test Strategy

- Unit tests for DAP message parsing/serialization
- Unit test for breakpoint matching logic
- Integration test: send initialize → launch → setBreakpoints → continue → verify stopped event

## Risks

- **Performance**: Breakpoint check on every statement. Mitigated: only check when `debug_state.is_some()` (single Option check, same pattern as coverage).
- **Deadlocks**: Condvar-based pause. Mitigated: always pair wait with notify, use timeout on wait.
- **stdout conflict**: Interpreter's println writes to stdout, but DAP protocol also uses stdout. Solution: redirect interpreter output to stderr or capture in buffer and send as DAP `output` events.
