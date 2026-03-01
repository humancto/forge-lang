# Forge Roadmap

> From interpreted language to compiled native binaries.
> Each milestone ships independently. Nothing breaks between releases.

---

## Current State: v0.3 — Feature Complete

**Status: Shipped**

| Component             | State                | Details                                                     |
| --------------------- | -------------------- | ----------------------------------------------------------- |
| Tree-walk interpreter | Complete             | Full feature support, 230+ builtins                         |
| Bytecode VM           | Complete             | Register-based, 50 opcodes, mark-sweep GC                   |
| JIT (Cranelift)       | Partial              | Integer-only functions, 20/50 opcodes                       |
| Standard library      | 16 modules           | 230+ functions across math, fs, crypto, db, http, npc, etc. |
| HTTP server           | Complete             | axum + tokio, decorator routing, WebSocket                  |
| HTTP client           | Complete             | reqwest, JSON, all methods                                  |
| Type checker          | Gradual              | Arity + type warnings, --strict mode                        |
| Tests                 | 488 Rust + 334 Forge | CI on Ubuntu + macOS                                        |
| Distribution          | Complete             | crates.io, Homebrew, curl installer, GitHub Releases        |

### What Works Today

```
forge run app.fg          # Tree-walk interpreter (default)
forge run --vm app.fg     # Bytecode VM
forge run --jit app.fg    # JIT for integer functions, VM fallback
forge build app.fg        # Prints bytecode stats (no file output)
```

### What Doesn't

- `forge build` doesn't produce a file
- JIT only handles integer math (no strings, objects, arrays)
- No AOT compilation
- No standalone binary output
- Profiler exists but is never wired in

---

## Milestone 1: v0.3 — Bytecode Persistence & VM Parity

**Goal:** `forge build app.fg` produces a `.fgc` file. `forge run app.fgc` loads and executes it. VM reaches feature parity with the interpreter.

**Why this first:** Everything after this depends on having a solid, serializable bytecode format and a VM that can run real programs.

### Phase 1.1 — Bytecode Serialization

| Task  | Files                       | Description                                                                                                                                                                              |
| ----- | --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.1.1 | `src/vm/bytecode.rs`        | Define binary format: magic bytes (`FGC\0`), version, chunk header (name, arity, registers, upvalue_count), constant table, code section, prototype table (recursive), line number table |
| 1.1.2 | `src/vm/serialize.rs` (new) | `serialize_chunk(chunk: &Chunk) -> Vec<u8>` — write chunk to binary. LEB128 for integers, length-prefixed UTF-8 for strings                                                              |
| 1.1.3 | `src/vm/serialize.rs`       | `deserialize_chunk(bytes: &[u8]) -> Result<Chunk, Error>` — read binary back to Chunk with validation                                                                                    |
| 1.1.4 | `src/main.rs`               | `forge build app.fg` writes `.fgc` file to disk                                                                                                                                          |
| 1.1.5 | `src/main.rs`               | `forge run app.fgc` detects compiled file, deserializes, and executes on VM                                                                                                              |
| 1.1.6 | Tests                       | Round-trip: compile → serialize → deserialize → execute, verify identical results                                                                                                        |

**Binary format spec:**

```
Offset  Size  Field
0       4     Magic: "FGC\0"
4       2     Version: major.minor (u8.u8)
6       1     Arity
7       1     Max registers
8       1     Upvalue count
9       var   Name (u16 length + UTF-8 bytes)
var     var   Constants table (u16 count + entries)
var     var   Code section (u32 count + u32[] instructions)
var     var   Line table (u32 count + u32[] lines)
var     var   Prototype table (u16 count + recursive Chunk encoding)
```

### Phase 1.2 — VM Feature Parity

| Task  | Files                                     | Description                                                                                                    |
| ----- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 1.2.1 | `src/vm/compiler.rs`                      | Compile `Stmt::Destructure` — array/object destructuring to register loads                                     |
| 1.2.2 | `src/vm/compiler.rs`                      | Compile `Stmt::TryCatch` fully — emit try-enter/try-exit/catch-jump instructions                               |
| 1.2.3 | `src/vm/bytecode.rs`, `src/vm/machine.rs` | Add `TryEnter`, `TryExit`, `ThrowError` opcodes                                                                |
| 1.2.4 | `src/vm/machine.rs`                       | Implement all builtin functions that exist in interpreter but not VM (method chaining, stdlib module dispatch) |
| 1.2.5 | `src/vm/machine.rs`                       | Wire up stdlib modules: `time.*`, `csv.*`, `exec.*` — currently missing from VM                                |
| 1.2.6 | `src/vm/compiler.rs`                      | Compile closures with upvalue capture (currently closures compile but don't capture outer variables)           |
| 1.2.7 | `src/vm/compiler.rs`, `src/vm/machine.rs` | Remove/implement `Pop` opcode, clean up `GetLocal`/`SetLocal` unused paths                                     |
| 1.2.8 | Tests                                     | Parity test suite: run same `.fg` programs on interpreter and VM, diff outputs                                 |

### Phase 1.3 — Profiler Integration

| Task  | Files                    | Description                                                                                                |
| ----- | ------------------------ | ---------------------------------------------------------------------------------------------------------- |
| 1.3.1 | `src/vm/jit/profiler.rs` | Extend profiler: track call count, total time, argument types per function                                 |
| 1.3.2 | `src/vm/machine.rs`      | Wire profiler into VM `call_value()` — record every function call                                          |
| 1.3.3 | `src/main.rs`            | `forge run --profile app.fg` dumps profiler stats after execution                                          |
| 1.3.4 | `src/vm/machine.rs`      | Auto-JIT: when profiler reports a function as hot (>100 calls) AND integer-only, JIT-compile it on the fly |

### Milestone 1 Deliverables

- [ ] `forge build app.fg → app.fgc` (binary bytecode file)
- [ ] `forge run app.fgc` (load + execute)
- [ ] VM runs all programs the interpreter can
- [ ] Profiler integrated, `--profile` flag works
- [ ] Auto-JIT for hot integer functions

### Commit Breakdown (M1)

```
m1-001: feat(bytecode): define binary format spec and magic bytes
m1-002: feat(serialize): implement chunk serialization to binary
m1-003: feat(serialize): implement chunk deserialization from binary
m1-004: feat(cli): forge build writes .fgc files
m1-005: feat(cli): forge run loads .fgc files
m1-006: test(serialize): round-trip serialization tests
m1-007: feat(vm): compile destructuring statements
m1-008: feat(vm): add TryEnter/TryExit/ThrowError opcodes
m1-009: feat(vm): implement full try-catch in compiler + VM
m1-010: feat(vm): wire all stdlib modules into VM dispatch
m1-011: feat(vm): implement upvalue capture for closures
m1-012: fix(vm): clean up unused Pop/GetLocal/SetLocal
m1-013: test(vm): interpreter-VM parity test suite
m1-014: feat(profiler): extend with timing and type tracking
m1-015: feat(vm): wire profiler into call_value
m1-016: feat(cli): add --profile flag
m1-017: feat(vm): auto-JIT for hot integer functions
```

---

## Milestone 2: v0.4 — JIT Expansion

**Goal:** JIT compiles functions with strings, arrays, objects, and floats — not just integers. Hot functions automatically promote to native code.

**Why this matters:** This is where performance goes from "Python-speed" to "Go-speed" for compute-heavy code. The JIT handles 90% of real functions, not just toy integer math.

### Phase 2.1 — NaN-Boxing Value Representation

| Task  | Files                    | Description                                                                                                                                                                          |
| ----- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2.1.1 | `src/vm/nanbox.rs` (new) | Implement NaN-boxed 64-bit value type: doubles use IEEE 754, non-doubles encoded in NaN payload bits. Tags: Int (48-bit), Bool, Null, Pointer (48-bit heap ref)                      |
| 2.1.2 | `src/vm/nanbox.rs`       | `encode_int(i64) -> u64`, `decode_int(u64) -> i64`, `encode_ptr(*const GcObject) -> u64`, `decode_ptr(u64) -> *const GcObject`, `is_float(u64) -> bool`, `is_int(u64) -> bool`, etc. |
| 2.1.3 | `src/vm/value.rs`        | Swap VM's `Value` enum to use NaN-boxed `u64` internally, keeping the same public API                                                                                                |
| 2.1.4 | `src/vm/machine.rs`      | Update all opcode handlers to work with NaN-boxed values                                                                                                                             |
| 2.1.5 | Tests                    | Exhaustive NaN-boxing tests: encode/decode round-trips for all types, edge cases (NaN, Infinity, max int, null pointer)                                                              |

**Why NaN-boxing:** One uniform 64-bit value that fits in a register. No heap allocation for primitives. No branch on type tag for arithmetic. This is what LuaJIT and JavaScriptCore use.

```
64-bit layout:
  Float:   standard IEEE 754 double (if not NaN)
  Int:     [1111...1][01][48-bit signed int]
  Bool:    [1111...1][10][0 or 1]
  Null:    [1111...1][11][0]
  Pointer: [1111...1][00][48-bit pointer]
```

### Phase 2.2 — JIT String Operations

| Task  | Files                      | Description                                                                                                                        |
| ----- | -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| 2.2.1 | `src/vm/jit/runtime.rs`    | Runtime bridge functions: `rt_string_concat(*str, *str) -> *str`, `rt_string_len(*str) -> i64`, `rt_string_eq(*str, *str) -> bool` |
| 2.2.2 | `src/vm/jit/ir_builder.rs` | JIT compile `Concat` opcode — emit call to `rt_string_concat`                                                                      |
| 2.2.3 | `src/vm/jit/ir_builder.rs` | JIT compile `Len` opcode for strings — emit call to `rt_string_len`                                                                |
| 2.2.4 | `src/vm/jit/ir_builder.rs` | JIT compile `Interpolate` opcode — emit loop calling `rt_string_concat`                                                            |
| 2.2.5 | `src/vm/jit/ir_builder.rs` | JIT compile `LoadConst` for string constants — emit pointer to interned string                                                     |
| 2.2.6 | Tests                      | JIT string tests: concat, interpolation, comparison, length                                                                        |

### Phase 2.3 — JIT Array and Object Operations

| Task  | Files                      | Description                                                                                                                                   |
| ----- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.3.1 | `src/vm/jit/runtime.rs`    | Runtime bridges: `rt_array_new(count) -> *arr`, `rt_array_get(*arr, idx) -> val`, `rt_array_set(*arr, idx, val)`, `rt_array_len(*arr) -> i64` |
| 2.3.2 | `src/vm/jit/runtime.rs`    | Runtime bridges: `rt_object_new() -> *obj`, `rt_object_get(*obj, *key) -> val`, `rt_object_set(*obj, *key, val)`                              |
| 2.3.3 | `src/vm/jit/ir_builder.rs` | JIT compile `NewArray`, `GetIndex`, `SetIndex` — emit calls to array runtime                                                                  |
| 2.3.4 | `src/vm/jit/ir_builder.rs` | JIT compile `NewObject`, `GetField`, `SetField` — emit calls to object runtime                                                                |
| 2.3.5 | `src/vm/jit/ir_builder.rs` | JIT compile `ExtractField` for pattern matching                                                                                               |
| 2.3.6 | Tests                      | JIT collection tests: array create/access/mutate, object create/field access                                                                  |

### Phase 2.4 — JIT Function Calls

| Task  | Files                      | Description                                                                                          |
| ----- | -------------------------- | ---------------------------------------------------------------------------------------------------- |
| 2.4.1 | `src/vm/jit/ir_builder.rs` | JIT compile `Call` for any JIT-compiled function (not just self-recursion) — emit direct native call |
| 2.4.2 | `src/vm/jit/ir_builder.rs` | JIT compile `Call` to non-JIT functions — emit call to `rt_call_interpreted` bridge                  |
| 2.4.3 | `src/vm/jit/ir_builder.rs` | JIT compile `Closure` — emit runtime call to allocate closure object                                 |
| 2.4.4 | `src/vm/jit/ir_builder.rs` | JIT compile `GetGlobal`/`SetGlobal` — emit calls to `rt_get_global`/`rt_set_global` bridges          |
| 2.4.5 | `src/vm/jit/runtime.rs`    | Implement `rt_call_interpreted`, `rt_get_global`, `rt_set_global` bridges                            |
| 2.4.6 | Tests                      | Cross-call tests: JIT calls JIT, JIT calls interpreter, interpreter calls JIT                        |

### Phase 2.5 — Tiered Compilation

| Task  | Files                      | Description                                                                                             |
| ----- | -------------------------- | ------------------------------------------------------------------------------------------------------- |
| 2.5.1 | `src/vm/jit/profiler.rs`   | Track argument types per call site to determine JIT eligibility                                         |
| 2.5.2 | `src/vm/machine.rs`        | Tier 0: Interpret. Tier 1: After 50 calls, check JIT eligibility. Tier 2: After 200 calls, JIT compile. |
| 2.5.3 | `src/vm/machine.rs`        | On-stack replacement (OSR) placeholder — mark hot loops for future optimization                         |
| 2.5.4 | `src/vm/jit/jit_module.rs` | Background JIT compilation — compile in separate thread, swap in when ready                             |
| 2.5.5 | Tests                      | Tiered compilation tests: verify functions promote correctly                                            |

### Milestone 2 Deliverables

- [ ] NaN-boxed value representation (uniform 64-bit)
- [ ] JIT handles strings, arrays, objects, floats
- [ ] JIT handles all function calls (not just self-recursion)
- [ ] Tiered compilation: interpret → profile → JIT
- [ ] Auto-JIT for any function type, not just integer-only
- [ ] Performance target: 10-20x over tree-walk for JIT-compiled code

### Commit Breakdown (M2)

```
m2-001: feat(nanbox): implement NaN-boxed 64-bit value representation
m2-002: feat(nanbox): encode/decode for int, float, bool, null, pointer
m2-003: refactor(vm): migrate Value to NaN-boxed internals
m2-004: refactor(vm): update all opcode handlers for NaN-boxed values
m2-005: test(nanbox): exhaustive encode/decode round-trip tests
m2-006: feat(jit): string runtime bridges (concat, len, eq)
m2-007: feat(jit): compile Concat, Len, Interpolate opcodes
m2-008: feat(jit): compile string LoadConst
m2-009: test(jit): string operation tests
m2-010: feat(jit): array runtime bridges (new, get, set, len)
m2-011: feat(jit): object runtime bridges (new, get, set)
m2-012: feat(jit): compile NewArray, GetIndex, SetIndex
m2-013: feat(jit): compile NewObject, GetField, SetField
m2-014: test(jit): collection operation tests
m2-015: feat(jit): compile cross-function Call
m2-016: feat(jit): compile Call to interpreted functions via bridge
m2-017: feat(jit): compile Closure and GetGlobal/SetGlobal
m2-018: test(jit): cross-call tests (JIT↔interpreter)
m2-019: feat(profiler): type-aware profiling for JIT eligibility
m2-020: feat(vm): tiered compilation (interpret → profile → JIT)
m2-021: test(tiered): promotion correctness tests
```

---

## Milestone 3: v0.5 — Type System & Safety

**Goal:** Type annotations are enforced. The compiler catches type errors before runtime. Interfaces work.

**Why here:** Before we compile to native binaries, we need types. Native code needs to know sizes, layouts, and calling conventions at compile time.

### Phase 3.1 — Type Inference Engine

| Task  | Files                | Description                                                                                                                                                                                                                                                                                                     |
| ----- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3.1.1 | `src/typechecker.rs` | Implement Hindley-Milner type inference with bidirectional checking. Infer types from usage when annotations are absent                                                                                                                                                                                         |
| 3.1.2 | `src/typechecker.rs` | Type environment: track variable types through scope, narrowing in if/match branches                                                                                                                                                                                                                            |
| 3.1.3 | `src/typechecker.rs` | Function signature inference: infer parameter and return types from body                                                                                                                                                                                                                                        |
| 3.1.4 | `src/typechecker.rs` | Validate annotated types match inferred types — emit error on mismatch                                                                                                                                                                                                                                          |
| 3.1.5 | `src/parser/ast.rs`  | Add `TypeAnnotation` enum: `Int`, `Float`, `String`, `Bool`, `Array(Box<TypeAnnotation>)`, `Object(Vec<(String, TypeAnnotation)>)`, `Function(Vec<TypeAnnotation>, Box<TypeAnnotation>)`, `Option(Box<TypeAnnotation>)`, `Result(Box<TypeAnnotation>, Box<TypeAnnotation>)`, `Generic(String)`, `Named(String)` |

### Phase 3.2 — Option<T> as a Real Type

| Task  | Files                                       | Description                                              |
| ----- | ------------------------------------------- | -------------------------------------------------------- |
| 3.2.1 | `src/interpreter/mod.rs`                    | Add `Value::None` variant (distinct from `Null`)         |
| 3.2.2 | `src/interpreter/mod.rs`, `src/vm/value.rs` | `Some(value)` wraps, `None` is the absent case           |
| 3.2.3 | `src/parser/parser.rs`                      | Parse `Option<T>` type annotations                       |
| 3.2.4 | `src/typechecker.rs`                        | Enforce: `Option<T>` values must be unwrapped before use |
| 3.2.5 | `src/typechecker.rs`                        | Null safety: variables without `Option` cannot be `None` |

### Phase 3.3 — Interface Satisfaction

| Task  | Files                | Description                                                                              |
| ----- | -------------------- | ---------------------------------------------------------------------------------------- |
| 3.3.1 | `src/typechecker.rs` | Check struct fields against interface method signatures                                  |
| 3.3.2 | `src/typechecker.rs` | Go-style implicit satisfaction: no `implements` keyword needed                           |
| 3.3.3 | `src/typechecker.rs` | Emit error when passing a struct to a function expecting an interface it doesn't satisfy |
| 3.3.4 | Tests                | Interface satisfaction tests across all valid/invalid combinations                       |

### Phase 3.4 — Generics

| Task  | Files                  | Description                                                                             |
| ----- | ---------------------- | --------------------------------------------------------------------------------------- |
| 3.4.1 | `src/parser/parser.rs` | Parse generic type parameters: `fn map<T, U>(arr: Array<T>, f: fn(T) -> U) -> Array<U>` |
| 3.4.2 | `src/typechecker.rs`   | Monomorphization: specialize generic functions for concrete types at call sites         |
| 3.4.3 | `src/typechecker.rs`   | Generic constraint checking: `<T: Comparable>`                                          |
| 3.4.4 | Tests                  | Generic type tests: identity, map, filter, custom generics                              |

### Milestone 3 Deliverables

- [ ] Type annotations enforced (gradual — unannotated code still works)
- [ ] Type inference for local variables and function signatures
- [ ] `Option<T>` with `Some`/`None` (no more raw null)
- [ ] Interface satisfaction checking
- [ ] Basic generics with monomorphization
- [ ] Type errors reported at compile time with source locations

### Commit Breakdown (M3)

```
m3-001: feat(types): TypeAnnotation enum in AST
m3-002: feat(types): bidirectional type inference engine
m3-003: feat(types): type environment with scope tracking
m3-004: feat(types): function signature inference
m3-005: feat(types): annotation validation (inferred vs declared)
m3-006: test(types): type inference test suite
m3-007: feat(types): Option<T> with Some/None variants
m3-008: feat(types): null safety enforcement
m3-009: feat(types): interface satisfaction checking
m3-010: feat(types): generic type parameter parsing
m3-011: feat(types): monomorphization
m3-012: test(types): generics and interface tests
```

---

## Milestone 4: v0.6 — Concurrency

**Goal:** `spawn` runs real concurrent tasks. Channels for communication. Async/await for I/O.

**Why here:** Before native compilation (M5), concurrency needs to work correctly at the VM level. Native code inherits whatever concurrency model the VM uses.

### Phase 4.1 — Green Threads on Tokio

| Task  | Files                | Description                                                                                                  |
| ----- | -------------------- | ------------------------------------------------------------------------------------------------------------ |
| 4.1.1 | `src/vm/green.rs`    | Replace synchronous scheduler with tokio::spawn. Each green thread gets its own register file and call stack |
| 4.1.2 | `src/vm/green.rs`    | Message passing: `send(thread_id, value)` and `receive() -> value` with bounded channels                     |
| 4.1.3 | `src/vm/machine.rs`  | `OpCode::Spawn` creates a real tokio task, returns a thread handle                                           |
| 4.1.4 | `src/vm/machine.rs`  | `OpCode::Yield` — cooperative yield point, returns control to scheduler                                      |
| 4.1.5 | `src/vm/bytecode.rs` | Add `Yield`, `Send`, `Receive` opcodes                                                                       |
| 4.1.6 | `src/vm/compiler.rs` | Compile `yield`, `send`, `receive` expressions                                                               |

### Phase 4.2 — Channels

| Task  | Files                     | Description                                                                                                     |
| ----- | ------------------------- | --------------------------------------------------------------------------------------------------------------- |
| 4.2.1 | `src/vm/channel.rs` (new) | Bounded MPMC channel: `Channel::new(capacity)`, `send(value)`, `recv() -> value`, `try_recv() -> Option<value>` |
| 4.2.2 | `src/vm/machine.rs`       | `channel(capacity)` builtin creates a channel pair                                                              |
| 4.2.3 | `src/vm/machine.rs`       | Select/multiplex: wait on multiple channels                                                                     |
| 4.2.4 | Tests                     | Concurrency tests: producer-consumer, fan-out, deadlock detection                                               |

### Phase 4.3 — Async I/O

| Task  | Files                   | Description                                                              |
| ----- | ----------------------- | ------------------------------------------------------------------------ |
| 4.3.1 | `src/vm/machine.rs`     | `await` keyword suspends current green thread, resumes on I/O completion |
| 4.3.2 | `src/runtime/client.rs` | `fetch()` returns a future that the scheduler can await                  |
| 4.3.3 | `src/stdlib/fs.rs`      | Async file I/O variants: `fs.read_async`, `fs.write_async`               |
| 4.3.4 | Tests                   | Async tests: concurrent fetches, file I/O with await                     |

### Milestone 4 Deliverables

- [ ] `spawn { }` runs real concurrent tasks on tokio
- [ ] Channels for inter-task communication
- [ ] `await` suspends tasks for I/O
- [ ] Select/multiplex on channels
- [ ] No data races (values are copied or moved, not shared)

---

## Milestone 5: v0.7 — AOT Native Compilation

**Goal:** `forge build app.fg` produces a standalone native binary. No Forge runtime needed to run it.

**Why this is the big one:** This is the jump from "scripting language" to "systems language." The binary runs without the `forge` CLI, ships as a single file, starts instantly, runs at near-Rust speed.

### Architecture

```
Source (.fg)
    │
    ▼
Lexer → Tokens → Parser → AST
    │
    ▼
Type Checker (from M3)
    │
    ▼
Bytecode Compiler (from M1)
    │
    ▼
Cranelift AOT Compiler ← NEW
    │
    ▼
Object File (.o)
    │
    ▼
Linker (cc/ld) + Forge Runtime Library
    │
    ▼
Standalone Native Binary
```

### Phase 5.1 — Runtime Library (libforge)

| Task  | Files                           | Description                                                                                                                                                                        |
| ----- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5.1.1 | `src/runtime/libforge.rs` (new) | Compile all builtin functions as `extern "C"` functions callable from native code: `forge_println(*str)`, `forge_array_new(count) -> *arr`, `forge_fetch(*url) -> *response`, etc. |
| 5.1.2 | `src/runtime/libforge.rs`       | GC interface for native code: `forge_gc_alloc(size) -> *ptr`, `forge_gc_root(ptr)`, `forge_gc_unroot(ptr)`                                                                         |
| 5.1.3 | `src/runtime/libforge.rs`       | String interning and management: `forge_string_new(*bytes, len) -> *str`, `forge_string_concat(*a, *b) -> *str`                                                                    |
| 5.1.4 | `src/runtime/libforge.rs`       | HTTP server bootstrap: `forge_server_start(port, routes) -> !` (the axum server loop)                                                                                              |
| 5.1.5 | Build                           | Compile libforge as a static library (`libforge.a`) that gets linked into output binaries                                                                                          |

### Phase 5.2 — AOT Cranelift Backend

| Task  | Files                         | Description                                                                                                                  |
| ----- | ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 5.2.1 | `src/vm/aot/mod.rs` (new)     | AOT compilation driver: takes `Chunk`, emits object file via `cranelift-object`                                              |
| 5.2.2 | `src/vm/aot/codegen.rs` (new) | Extend `ir_builder.rs` logic for AOT: all opcodes compiled (not just integer subset). Unknown/complex ops call into libforge |
| 5.2.3 | `src/vm/aot/codegen.rs`       | Stack maps for GC: emit metadata so the GC knows which registers hold pointers at each safe point                            |
| 5.2.4 | `src/vm/aot/codegen.rs`       | Exception handling: translate try-catch to landing pads or setjmp/longjmp                                                    |
| 5.2.5 | `src/vm/aot/linker.rs` (new)  | Invoke system linker (`cc`) to link object file + libforge.a → final binary                                                  |
| 5.2.6 | `src/main.rs`                 | `forge build --native app.fg` → `app` (or `app.exe` on Windows)                                                              |

### Phase 5.3 — HTTP Server in Native Binaries

| Task  | Files                     | Description                                                                                                    |
| ----- | ------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 5.3.1 | `src/runtime/libforge.rs` | `forge_register_route(method, path, fn_ptr)` — register a native function as an HTTP handler                   |
| 5.3.2 | `src/vm/aot/codegen.rs`   | Compile decorated functions (`@get`, `@post`) — emit calls to `forge_register_route` in the generated `main()` |
| 5.3.3 | `src/runtime/libforge.rs` | Route dispatch: axum handler calls native function pointer, converts JSON ↔ Forge values                       |
| 5.3.4 | Tests                     | Build example API server to native binary, run it, `curl` test the endpoints                                   |

### Phase 5.4 — Cross-Compilation

| Task  | Files                           | Description                                                       |
| ----- | ------------------------------- | ----------------------------------------------------------------- |
| 5.4.1 | `src/vm/aot/mod.rs`             | Target triple support: `forge build --target x86_64-linux app.fg` |
| 5.4.2 | `.github/workflows/release.yml` | Release workflow builds pre-compiled libforge.a for each target   |
| 5.4.3 | `src/vm/aot/linker.rs`          | Cross-linker detection: find appropriate linker for target        |

### Milestone 5 Deliverables

- [ ] `forge build --native app.fg → app` (standalone binary)
- [ ] Binary includes: compiled user code + libforge runtime
- [ ] HTTP servers work as standalone binaries
- [ ] Startup time: < 10ms (vs ~100ms for interpreter)
- [ ] Performance: within 2-5x of equivalent Rust code
- [ ] Cross-compilation to Linux/macOS (x86_64 + ARM)

### Commit Breakdown (M5)

```
m5-001: feat(runtime): libforge extern C function stubs
m5-002: feat(runtime): GC interface for native code
m5-003: feat(runtime): string management API
m5-004: feat(runtime): HTTP server bootstrap for native binaries
m5-005: build: compile libforge as static library
m5-006: feat(aot): AOT compilation driver with cranelift-object
m5-007: feat(aot): full opcode codegen (calls into libforge for complex ops)
m5-008: feat(aot): GC stack maps at safe points
m5-009: feat(aot): try-catch as landing pads
m5-010: feat(aot): system linker invocation
m5-011: feat(cli): forge build --native produces binary
m5-012: feat(aot): HTTP route registration in generated code
m5-013: feat(aot): compile decorated server functions
m5-014: test(aot): end-to-end native API server test
m5-015: feat(aot): cross-compilation target support
m5-016: ci: release workflow for libforge.a per target
```

---

## Milestone 6: v0.8 — Package Ecosystem

**Goal:** `forge.toml` declares dependencies. `forge install` resolves and downloads them. `forge publish` shares packages.

### Phase 6.1 — Package Manifest

| Task  | Description                                                                         |
| ----- | ----------------------------------------------------------------------------------- |
| 6.1.1 | `forge.toml` schema: name, version, description, dependencies, scripts, entry point |
| 6.1.2 | `forge install <name>` downloads from registry, resolves versions                   |
| 6.1.3 | `forge.lock` lockfile for reproducible builds                                       |
| 6.1.4 | `forge publish` packages and uploads to registry                                    |

### Phase 6.2 — Module Resolution

| Task  | Description                                                          |
| ----- | -------------------------------------------------------------------- |
| 6.2.1 | `import { foo } from "bar"` resolves from `forge_modules/` or stdlib |
| 6.2.2 | Cycle detection in module imports                                    |
| 6.2.3 | Module caching (don't re-parse imported modules)                     |

### Milestone 6 Deliverables

- [ ] `forge.toml` dependency management
- [ ] Package registry (forge-packages.dev)
- [ ] `forge install`, `forge publish`, `forge update`
- [ ] Module resolution with cycle detection

---

## Milestone 7: v0.9 — Developer Experience

**Goal:** World-class tooling. IDE support, debugger, documentation generator.

### Tasks

| Task | Description                                                           |
| ---- | --------------------------------------------------------------------- |
| 7.1  | LSP: completion, hover docs, go-to-definition, find references        |
| 7.2  | VS Code extension with syntax highlighting, snippets, LSP integration |
| 7.3  | `forge doc` generates HTML documentation from source comments         |
| 7.4  | `forge bench` benchmarking with statistical analysis                  |
| 7.5  | DAP debugger: breakpoints, step-through, variable inspection          |
| 7.6  | REPL syntax highlighting                                              |
| 7.7  | `forge watch` — file watcher that re-runs on save                     |

---

## Milestone 8: v1.0 — Production Release

**Goal:** Stable, documented, battle-tested. Ready for real-world production use.

### Tasks

| Task | Description                                                       |
| ---- | ----------------------------------------------------------------- |
| 8.1  | Language specification document (formal grammar, semantics)       |
| 8.2  | Stable API guarantee — no breaking changes without major version  |
| 8.3  | Windows support (native compilation + installer)                  |
| 8.4  | Docker image: `docker run forge app.fg`                           |
| 8.5  | WebAssembly target: `forge build --target wasm app.fg → app.wasm` |
| 8.6  | Performance benchmarks published (vs Go, Python, Node, Lua)       |
| 8.7  | Security audit                                                    |
| 8.8  | "The Forge Book" — comprehensive documentation                    |

---

## Milestone Dependency Graph

```
M1 (Bytecode Persistence + VM Parity)
 │
 ├──→ M2 (JIT Expansion)
 │     │
 │     └──→ M5 (AOT Native Compilation)
 │           │
 │           └──→ M8 (Production / WASM)
 │
 ├──→ M3 (Type System)
 │     │
 │     └──→ M5 (types inform native code layout)
 │
 ├──→ M4 (Concurrency)
 │     │
 │     └──→ M5 (native binaries need concurrency model)
 │
 ├──→ M6 (Package Ecosystem) ← can start after M1
 │
 └──→ M7 (Developer Experience) ← can start after M1
```

### Parallelism Opportunities

- **M3 (Types) and M2 (JIT)** can run in parallel after M1
- **M6 (Packages) and M7 (DX)** can run in parallel, independent of M2-M5
- **M4 (Concurrency)** depends only on M1

---

## Performance Targets

| Benchmark        | v0.3 (current) | M1 target    | M2 target    | M5 target       |
| ---------------- | -------------- | ------------ | ------------ | --------------- |
| fib(35)          | ~4s (interp)   | ~0.5s (VM)   | ~0.05s (JIT) | ~0.02s (native) |
| string concat 1M | ~2s            | ~1.5s        | ~0.3s        | ~0.1s           |
| HTTP req/sec     | ~5k            | ~8k          | ~15k         | ~50k+           |
| Startup time     | ~100ms         | ~50ms (.fgc) | ~50ms        | ~5ms (native)   |
| Binary size      | N/A            | N/A          | N/A          | ~5-15MB         |

---

## Design Principles (Unchanged)

1. **Internet-native** — HTTP, databases, crypto are language primitives
2. **Human-readable** — natural syntax is first-class
3. **Errors are values** — no exceptions, no invisible control flow
4. **Immutable by default** — `let` is immutable, `let mut` opts in
5. **No null** — `Option<T>` is the only nullable path
6. **No unsafe** — the entire codebase remains safe Rust
7. **No OOP** — structs + interfaces, no classes, no inheritance
8. **Compilation is progressive** — interpret → bytecode → JIT → native, all paths always work

---

## How to Influence the Roadmap

- Open an issue with the `feature-request` label
- Submit an RFC in the `rfcs/` directory
- Join the discussion on existing roadmap issues

Priorities are driven by what makes Forge more useful for building real internet software.
