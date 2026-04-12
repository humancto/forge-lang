# Phase 3.1 — Async VM Runtime: Implementation Plan

Written: 2026-04-11
Status: **Draft v3 — revised after two expert reviews**

---

## Goal

Make `spawn`, `await` work in `--vm` mode with the same semantics as the interpreter. This removes the biggest remaining gap between the two execution engines. `schedule` and `watch` are deferred to Phase 3.5.

---

## Current State

| Feature                | Interpreter                                                         | VM                                                                                 |
| ---------------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `spawn { ... }` (stmt) | `std::thread::spawn` + deep-cloned env, fire-and-forget             | Compiles closure + `OpCode::Spawn`, runs **synchronously inline** via `call_value` |
| `spawn { ... }` (expr) | Returns `TaskHandle(Arc<Mutex<Option<Value>>, Condvar>)`            | Compiles body **inline** (not as closure), returns `null`                          |
| `await expr`           | Blocks on `Condvar::wait` if `TaskHandle`, passes through otherwise | Passthrough — compiles inner expr only, no blocking                                |
| `schedule every N { }` | `std::thread::spawn` infinite loop + sleep                          | **No-op** in compiler                                                              |
| `watch "file" { }`     | `std::thread::spawn` polling mtime                                  | **No-op** in compiler                                                              |

`main.rs` compatibility checker (lines 390-590) rejects `spawn` (expr), `await`, `schedule`, and `watch` for `--vm` mode.

`src/vm/green.rs` has green thread infrastructure but is entirely unused — `spawn_sync` just calls `call_value` synchronously. Stays dead code.

---

## Design Decisions

### 1. Threading model: `std::thread` (matching interpreter)

The interpreter uses `std::thread::spawn` + `Condvar` (not tokio). We match this:

- No new dependencies
- Same concurrency semantics (true OS-thread parallelism)
- `green.rs` cooperative scheduler stays dead code — different design direction for the future

### 2. `SharedValue` for cross-thread data transfer

The VM's `Value::Obj(GcRef)` is an index into a specific VM's `Gc` heap. A `GcRef` from one VM is meaningless (and dangerous) in another. We need a GC-free intermediate representation for values crossing thread boundaries.

`SharedValue` is an owned enum that mirrors `Value` but contains no `GcRef`s:

```rust
#[derive(Clone)]
pub enum SharedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    String(String),
    Array(Vec<SharedValue>),
    Object(IndexMap<String, SharedValue>),
    ResultOk(Box<SharedValue>),
    ResultErr(Box<SharedValue>),
}
```

Used in two places:

- **Spawn result slot:** `Arc<(Mutex<Option<SharedValue>>, Condvar)>` — spawned thread converts `Value` -> `SharedValue` before storing; awaiting thread converts `SharedValue` -> `Value` (allocating in its own GC)
- **Globals/upvalue transfer:** when forking a VM for spawn, non-function globals and upvalue values are converted through `SharedValue` to re-allocate in the new GC

**Critical rule:** Functions, closures, and native functions map to `SharedValue::Null`. During `fork_for_spawn`, globals that convert to `SharedValue::Null` must be **skipped** (not copied), so they don't overwrite the child VM's freshly-registered builtins. See Step 3.

### 3. VM forking: `VM::new()` + selective copy (NOT full heap clone)

The interpreter's `spawn_task` (line 2798) creates `Interpreter::new()` + `env.deep_clone()` — it clones each scope's HashMap (new Mutex wrappers around cloned data), NOT a deep-clone of all heap values.

The VM equivalent:

1. `VM::new()` — gets fresh builtins, empty `jit_cache` (solves the `Send` problem), fresh GC
2. Copy non-function globals by converting each `Value` through `SharedValue` and re-allocating in the new GC. **Skip any global where `value_to_shared` returns `SharedValue::Null`** — this preserves the child's own builtins from `register_builtins()`
3. Copy `method_tables`, `static_methods`, `embedded_fields`, `struct_defaults` the same way (skipping Null conversions for method values)
4. Re-allocate the closure + its upvalues in the new GC (the `Arc<Chunk>` is safely shared)

This avoids the GC remapping table entirely.

### 4. `VM` is not `Send` — solved by construction

`VM` contains `jit_cache: HashMap<String, JitEntry>` where `JitEntry` has `ptr: *const u8`. Raw pointers are not `Send`, so `std::thread::spawn(move || spawn_vm...)` won't compile.

Solution: `fork_for_spawn` creates via `VM::new()` which has empty `jit_cache`. We wrap the forked VM in a newtype with `unsafe impl Send`:

```rust
struct SendableVM(VM);
unsafe impl Send for SendableVM {}
```

This is safe because the forked VM has no JIT pointers. The `unsafe impl` scope is narrow.

### 5. TaskHandle type: `Arc<(Mutex<Option<SharedValue>>, Condvar)>`

The condvar slot holds `Option<SharedValue>` (NOT `Option<Value>`). This enforces at the type level that no `GcRef`s leak across threads. The `ObjKind::TaskHandle` variant wraps this Arc. `Gc::alloc` takes `ObjKind` (it wraps in `GcObject` internally).

### 6. Closure + upvalue transfer

When spawning, the closure (a `Value::Obj` pointing to `ObjClosure` in parent GC) must be re-created in the spawned VM's GC:

1. Read the `ObjClosure` from the parent GC
2. The `ObjFunction.chunk` is `Arc<Chunk>` — safe to share (just `Arc::clone`)
3. For each upvalue `GcRef`: read the `ObjUpvalue.value` from parent GC, convert to `SharedValue`, re-allocate as new `ObjUpvalue` in spawned GC
4. Create new `ObjClosure` in spawned GC with the shared chunk and new upvalue refs

**Critical:** The spawn sub-compiler must populate `parent_locals` and `parent_upvalues` (from `c.snapshot_locals()` / `c.snapshot_upvalues()`) so variable capture works. Without this, every variable reference inside spawn falls through to `GetGlobal` and upvalue transfer is dead code. See Step 6.

### 7. `call_value` return type

`VM::call_value` returns `Result<Value, VMError>` (NOT `Result<Option<Value>>`). The spawn thread handler must match `Ok(v)`, not `Ok(Some(v))`.

### 8. Double-await semantics

`guard.take()` in the Await handler means a second `await` on the same TaskHandle returns `Null`. This matches the interpreter's behavior (line 2300). Documented as intentional, not a bug.

---

## Implementation Steps

### Step 1: `SharedValue` enum + conversions

**File:** `src/vm/value.rs`

Add `SharedValue` enum (defined above) and two conversion functions as free functions (only need `&Gc`, not full `&VM`):

```rust
/// Convert a VM Value to a SharedValue (owns all data, no GcRefs).
pub fn value_to_shared(gc: &Gc, val: &Value) -> SharedValue { ... }

/// Convert a SharedValue back to a VM Value (allocates in target GC).
pub fn shared_to_value(gc: &mut Gc, sv: &SharedValue) -> Value { ... }
```

Making these free functions taking `&Gc` / `&mut Gc` (not `impl VM` methods) avoids borrow conflicts when called inside loops that borrow other VM fields.

Conversion rules:

- `Int/Float/Bool/Null` → direct mapping
- `Obj(r)` → read from GC, recursively convert children
- `ObjKind::String(s)` → `SharedValue::String(s.clone())`
- `ObjKind::Array(items)` → `SharedValue::Array(items.map(|v| value_to_shared(gc, v)))`
- `ObjKind::Object(map)` → `SharedValue::Object(map entries mapped)`
- `ObjKind::ResultOk/Err(v)` → `SharedValue::ResultOk/Err(Box::new(...))`
- `ObjKind::Function/Closure/NativeFunction/Upvalue/TaskHandle` → `SharedValue::Null` (not transferable)

**Estimated: ~70 lines**

### Step 2: `ObjKind::TaskHandle` + `OpCode::Await`

**File:** `src/vm/value.rs`

```rust
// Add to ObjKind enum:
TaskHandle(Arc<(std::sync::Mutex<Option<SharedValue>>, std::sync::Condvar)>),
```

Update match arms:

- `GcObject::display` → `"<task>"`
- `GcObject::type_name` → `"TaskHandle"`
- `GcObject::to_json_string` → `"\"<task>\""`
- `GcObject::trace` → no children to trace (SharedValue has no GcRefs)
- `GcObject::equals` → `false` (identity only)
- `Value::is_truthy` → caught by existing `_ => true` wildcard — TaskHandles are truthy (matches interpreter where TaskHandle is a non-null value). Intentional.

**File:** `src/vm/bytecode.rs`

```rust
Await, // A=dst, B=src (if TaskHandle: block + deserialize; else: pass through)
```

Appended after `PopTimeout`. Discriminant is contiguous (34th variant, well within 8-bit limit of 256). The `execute` loop's `unsafe { transmute(op) }` is safe as long as no gaps exist in the enum — Rust's automatic `#[repr(u8)]` numbering guarantees this for appended variants.

**Estimated: ~25 lines**

### Step 3: `SendableVM` + `fork_for_spawn` + `transfer_closure`

**File:** `src/vm/machine.rs`

```rust
/// Wrapper for sending a VM to another thread.
/// Safe because forked VMs have empty jit_cache (no raw pointers).
struct SendableVM(VM);
unsafe impl Send for SendableVM {}

impl VM {
    /// Create a new VM for a spawn thread with copies of this VM's state.
    /// Calls VM::new() for fresh builtins + empty jit_cache, then copies
    /// non-function globals and struct metadata from the parent.
    fn fork_for_spawn(&self) -> SendableVM {
        let mut child = VM::new(); // fresh builtins, empty jit_cache, new GC

        // Copy non-function globals (re-allocate heap objects in child's GC).
        // CRITICAL: Skip globals that convert to SharedValue::Null (functions,
        // closures, native functions) — these would overwrite the child's
        // freshly-registered builtins from VM::new().
        for (name, val) in &self.globals {
            let shared = value_to_shared(&self.gc, val);
            if matches!(shared, SharedValue::Null) && !matches!(val, Value::Null) {
                continue; // was a function/closure/native — skip, keep child's builtin
            }
            let child_val = shared_to_value(&mut child.gc, &shared);
            child.globals.insert(name.clone(), child_val);
        }

        // Copy struct metadata (method_tables values are IndexMap<String, Value>)
        for (name, methods) in &self.method_tables {
            let mut child_methods = IndexMap::new();
            for (k, v) in methods {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !matches!(v, Value::Null) {
                    continue;
                }
                child_methods.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.method_tables.insert(name.clone(), child_methods);
        }
        for (name, methods) in &self.static_methods {
            let mut child_methods = IndexMap::new();
            for (k, v) in methods {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !matches!(v, Value::Null) {
                    continue;
                }
                child_methods.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.static_methods.insert(name.clone(), child_methods);
        }

        // embedded_fields is HashMap<String, Vec<(String, String)>> — all Strings, no Values
        child.embedded_fields = self.embedded_fields.clone();

        // struct_defaults has Values that need re-allocation
        for (name, defaults) in &self.struct_defaults {
            let mut child_defaults = IndexMap::new();
            for (k, v) in defaults {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !matches!(v, Value::Null) {
                    continue;
                }
                child_defaults.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.struct_defaults.insert(name.clone(), child_defaults);
        }

        SendableVM(child)
    }

    /// Re-create a closure from parent GC in a child VM's GC.
    /// The Arc<Chunk> is shared; upvalue values are copied via SharedValue.
    fn transfer_closure(&self, closure_ref: GcRef, child: &mut VM) -> Value {
        let obj = self.gc.get(closure_ref).expect("BUG: closure ref invalid");
        match &obj.kind {
            ObjKind::Closure(c) => {
                let function = ObjFunction {
                    name: c.function.name.clone(),
                    chunk: Arc::clone(&c.function.chunk),
                };
                // Transfer upvalues: read each from parent GC, convert through
                // SharedValue, re-allocate in child GC
                let mut child_upvalues = Vec::new();
                for uv_ref in &c.upvalues {
                    let uv_val = self.gc.get(*uv_ref)
                        .and_then(|o| match &o.kind {
                            ObjKind::Upvalue(uv) => Some(&uv.value),
                            _ => None,
                        })
                        .cloned()
                        .unwrap_or(Value::Null);
                    let shared = value_to_shared(&self.gc, &uv_val);
                    let child_val = shared_to_value(&mut child.gc, &shared);
                    let child_uv = child.gc.alloc(
                        ObjKind::Upvalue(ObjUpvalue { value: child_val })
                    );
                    child_upvalues.push(child_uv);
                }
                let closure = ObjClosure { function, upvalues: child_upvalues };
                let r = child.gc.alloc(ObjKind::Closure(closure));
                Value::Obj(r)
            }
            ObjKind::Function(f) => {
                let function = ObjFunction {
                    name: f.name.clone(),
                    chunk: Arc::clone(&f.chunk),
                };
                let r = child.gc.alloc(ObjKind::Function(function));
                Value::Obj(r)
            }
            _ => Value::Null, // shouldn't happen — spawn always creates a closure
        }
    }
}
```

**Estimated: ~110 lines**

### Step 4: Real `Spawn` opcode handler

**File:** `src/vm/machine.rs`

Replace the synchronous handler:

```rust
// Before (line 1444):
OpCode::Spawn => {
    let closure_val = self.registers[base + a as usize].clone();
    self.call_value(closure_val, vec![])?;
}

// After:
OpCode::Spawn => {
    let closure_val = self.registers[base + a as usize].clone();
    let result_slot: Arc<(Mutex<Option<SharedValue>>, Condvar)> =
        Arc::new((Mutex::new(None), Condvar::new()));
    let slot_clone = result_slot.clone();

    // Fork VM and transfer closure to child's GC
    let mut sendable = self.fork_for_spawn();
    let child_closure = if let Value::Obj(r) = &closure_val {
        self.transfer_closure(*r, &mut sendable.0)
    } else {
        Value::Null
    };

    std::thread::spawn(move || {
        let vm = &mut sendable.0;
        // call_value returns Result<Value, VMError> (NOT Option<Value>)
        let val = match vm.call_value(child_closure, vec![]) {
            Ok(v) => value_to_shared(&vm.gc, &v),
            Err(e) => {
                eprintln!("spawn error: {}", e.message);
                SharedValue::Null
            }
        };
        if let Ok(mut guard) = slot_clone.0.lock() {
            *guard = Some(val);
            slot_clone.1.notify_all();
        }
    });

    // Store TaskHandle in register A (overwrites consumed closure — safe,
    // closure was already read and transferred before this point)
    let handle = self.gc.alloc(ObjKind::TaskHandle(result_slot));
    self.registers[base + a as usize] = Value::Obj(handle);
}
```

**GC safety:** Between reading `closure_val` and writing the TaskHandle, we call `fork_for_spawn` and `transfer_closure` which read from `self.gc` but don't allocate on the parent (child allocations are on `child.gc`). The `self.gc.alloc()` at the end could trigger GC, but the parent register still holds `Value::Obj(closure_ref)` which roots the closure. After alloc, we immediately overwrite with the handle ref.

**Stmt::Spawn note:** For fire-and-forget `Stmt::Spawn` (line 1446), the compiler frees the register immediately after `OpCode::Spawn`, so the TaskHandle is discarded. The spawned thread runs independently. This matches interpreter behavior where `Stmt::Spawn` calls `spawn_task` and drops the handle.

**Estimated: ~35 lines**

### Step 5: `Await` opcode handler

**File:** `src/vm/machine.rs`

```rust
OpCode::Await => {
    let src = self.registers[base + b as usize].clone();
    let result = if let Value::Obj(r) = &src {
        if let Some(obj) = self.gc.get(*r) {
            if let ObjKind::TaskHandle(slot) = &obj.kind {
                let slot = slot.clone(); // Arc clone — drops the &self.gc borrow
                let (lock, cvar) = &*slot;
                let mut guard = lock.lock()
                    .map_err(|_| VMError::new("await: spawned task panicked"))?;
                while guard.is_none() {
                    guard = cvar.wait(guard)
                        .map_err(|_| VMError::new("await: wait interrupted"))?;
                }
                // guard.take() means second await on same handle returns Null
                // (matches interpreter behavior at interpreter/mod.rs:2300)
                let shared = guard.take().unwrap_or(SharedValue::Null);
                shared_to_value(&mut self.gc, &shared)
            } else {
                src // not a TaskHandle, pass through
            }
        } else {
            src
        }
    } else {
        src // primitive, pass through
    };
    self.registers[base + a as usize] = result;
}
```

**Borrow safety:** `slot.clone()` (Arc clone) is done while holding `&self.gc` borrow (from `self.gc.get`). The clone copies the Arc, then we drop the `obj` / `gc.get` borrow by falling out of the `if let` blocks. `shared_to_value(&mut self.gc, ...)` borrows `self.gc` mutably — this is safe because the `obj` borrow is no longer held (we only have the Arc clone). However, the code as written still holds `obj` when calling `slot.clone()`. We must restructure:

```rust
OpCode::Await => {
    let src = self.registers[base + b as usize].clone();
    // Extract the Arc first, releasing the GC borrow
    let maybe_slot = if let Value::Obj(r) = &src {
        self.gc.get(*r).and_then(|obj| {
            if let ObjKind::TaskHandle(slot) = &obj.kind {
                Some(slot.clone()) // Arc clone
            } else {
                None
            }
        })
    } else {
        None
    };
    // Now GC borrow is released — safe to call shared_to_value(&mut self.gc)
    let result = if let Some(slot) = maybe_slot {
        let (lock, cvar) = &*slot;
        let mut guard = lock.lock()
            .map_err(|_| VMError::new("await: spawned task panicked"))?;
        while guard.is_none() {
            guard = cvar.wait(guard)
                .map_err(|_| VMError::new("await: wait interrupted"))?;
        }
        let shared = guard.take().unwrap_or(SharedValue::Null);
        shared_to_value(&mut self.gc, &shared)
    } else {
        src // not a TaskHandle or not an Obj — pass through
    };
    self.registers[base + a as usize] = result;
}
```

**Estimated: ~30 lines**

### Step 6: Compiler fixes

**File:** `src/vm/compiler.rs`

**6a. Fix `Expr::Spawn`** (line 1679) — compile as closure + Spawn opcode:

```rust
// Before:
Expr::Spawn(body) => {
    // VM spawn: compile as synchronous call for now (VM concurrency is M4.3)
    for stmt in body { ... }
    c.emit(encode_abc(OpCode::LoadNull, dst, 0, 0), 0);
}

// After:
Expr::Spawn(body) => {
    // CRITICAL: snapshot parent context so spawn closure can capture variables.
    // Without this, all variable refs inside spawn fall through to GetGlobal.
    let parent_locals = c.snapshot_locals();
    let parent_upvalues = c.snapshot_upvalues();

    let mut sc = Compiler::new("<spawn>");
    sc.parent_locals = parent_locals;
    sc.parent_upvalues = parent_upvalues;
    sc.current_line = c.current_line;
    sc.begin_scope();
    for s in body {
        sc.current_line = s.line;
        compile_stmt(&mut sc, &s.stmt)?;
    }
    // Explicit ReturnNull — Stmt::Spawn (line 1454) does this.
    // Without it, the closure relies on ip-past-end fallthrough which is fragile.
    sc.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 0);
    sc.chunk.max_registers = sc.max_register;
    let proto = c.chunk.prototypes.len() as u16;
    c.chunk.prototypes.push(sc.chunk);
    c.emit(encode_abx(OpCode::Closure, dst, proto), 0);
    c.emit(encode_abc(OpCode::Spawn, dst, 0, 0), 0);
    // dst now holds TaskHandle (Spawn opcode overwrites register A)
}
```

**6b. Fix `Stmt::Spawn`** (line 1446) — add parent context for upvalue capture:

```rust
// Before:
Stmt::Spawn { body } => {
    let mut sc = Compiler::new("<spawn>");
    sc.current_line = c.current_line;
    // ...
}

// After:
Stmt::Spawn { body } => {
    let parent_locals = c.snapshot_locals();
    let parent_upvalues = c.snapshot_upvalues();

    let mut sc = Compiler::new("<spawn>");
    sc.parent_locals = parent_locals;
    sc.parent_upvalues = parent_upvalues;
    sc.current_line = c.current_line;
    // ... rest unchanged
}
```

**6c. Split `Expr::Await`** (line 1676) — emit Await opcode:

```rust
// Before:
Expr::Await(inner) | Expr::Must(inner) | Expr::Freeze(inner) | Expr::Ask(inner) => {
    compile_expr(c, inner, dst)?;
}

// After:
Expr::Await(inner) => {
    let src = c.alloc_reg();
    compile_expr(c, inner, src)?;
    c.emit(encode_abc(OpCode::Await, dst, src, 0), c.current_line);
    c.free_to(src);
}
Expr::Must(inner) | Expr::Freeze(inner) | Expr::Ask(inner) => {
    compile_expr(c, inner, dst)?;
}
```

**Estimated: ~35 lines**

### Step 7: Remove VM compatibility rejections

**File:** `src/main.rs`

In `collect_vm_incompatible_expr`:

- Remove the `Expr::Spawn` branch (lines 576-583) — no longer insert `"spawn expressions"`
- Remove the `Expr::Await` branch (lines 567-570) — no longer insert `"await expressions"`
- `Stmt::Spawn` (line 452) already just recurses into body without inserting an issue — no change needed

Keep `schedule blocks` and `watch blocks` rejected (Phase 3.5).

**Estimated: ~10 lines removed**

### Step 8: Tests

**File:** `src/vm/mod.rs`

12 tests covering the full async surface:

| #   | Test                            | What it verifies                                              |
| --- | ------------------------------- | ------------------------------------------------------------- |
| 1   | `vm_spawn_returns_task_handle`  | `typeof(spawn { 42 })` -> "TaskHandle"                        |
| 2   | `vm_spawn_display`              | `spawn { 1 }` displays as "<task>"                            |
| 3   | `vm_await_spawn_gets_value`     | `await spawn { return 42 }` -> 42                             |
| 4   | `vm_await_non_task_passthrough` | `await 99` -> 99                                              |
| 5   | `vm_spawn_stmt_fire_and_forget` | `spawn { let x = 1 }` doesn't crash                           |
| 6   | `vm_multiple_spawns_await`      | Spawn 3 tasks, await all, verify all results                  |
| 7   | `vm_spawn_with_computation`     | Spawn does loop/math work, await gets result                  |
| 8   | `vm_await_string_result`        | Spawn returns string, crosses thread boundary via SharedValue |
| 9   | `vm_spawn_captures_variable`    | Spawn closure captures outer variable via upvalue             |
| 10  | `vm_nested_spawn`               | Spawn inside spawn — recursive fork_for_spawn                 |
| 11  | `vm_spawn_error_no_crash`       | Spawn block throws error, parent awaits and gets null         |
| 12  | `vm_spawn_returns_object`       | Spawn returns `{ a: 1, b: "hi" }`, verifies object transfer   |

**File:** `src/vm/serialize.rs`

Add `Spawn` and `Await` opcodes to the `round_trip_all_instruction_opcodes` test (line 545) to ensure bytecode serialization covers the new opcodes.

**Estimated: ~165 lines**

---

## What This Does NOT Cover (deferred to 3.5)

- `schedule every N { }` — needs runtime plan / host infrastructure (`runtime::host::launch` equivalent for VM)
- `watch "file" { }` — needs file polling infrastructure
- Both depend on spawn/await primitives from this phase

---

## Risk Assessment

| Risk                                       | Mitigation                                                                                             |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| `VM` not `Send` (JitEntry has raw ptr)     | `fork_for_spawn` returns `SendableVM` wrapping VM with empty `jit_cache`                               |
| Cross-thread GcRef dangling                | `SharedValue` enforced at type level — condvar slot is `Option<SharedValue>`                           |
| Closure GcRef passed to child VM           | `transfer_closure` re-allocates closure + upvalues in child GC; `Arc<Chunk>` shared                    |
| Spawn closure can't capture variables      | Sub-compiler gets `parent_locals`/`parent_upvalues` from `c.snapshot_locals()`/`c.snapshot_upvalues()` |
| `fork_for_spawn` overwrites child builtins | Skip globals where `value_to_shared` returns Null but original value wasn't Null                       |
| GC during spawn setup                      | Closure rooted in register A until `gc.alloc(TaskHandle)` overwrites it                                |
| `call_value` return type                   | Returns `Result<Value, VMError>` — match `Ok(v)`, not `Ok(Some(v))`                                    |
| Spawned thread panics                      | `Condvar::wait` returns poisoned Mutex error -> VMError                                                |
| Nested spawn                               | Recursive `fork_for_spawn` works — each child is a full `VM::new()`                                    |
| Missing struct metadata                    | `fork_for_spawn` copies `method_tables`, `static_methods`, `embedded_fields`, `struct_defaults`        |
| Spawn returns function/closure             | `value_to_shared` maps functions -> `SharedValue::Null` (not transferable)                             |
| `Expr::Spawn` missing ReturnNull           | Explicit `ReturnNull` emitted (matching `Stmt::Spawn` at compiler.rs:1454)                             |
| Double-await returns Null                  | `guard.take()` — intentional, matches interpreter at mod.rs:2300                                       |
| Borrow conflict in Await                   | Extract Arc clone first, release GC borrow, then call `shared_to_value`                                |
| Spawned output lost                        | VM `println` writes to both stdout AND `self.output` — stdout output appears                           |
| Bytecode serialization                     | `serialize.rs` operates on raw u32 — new opcodes round-trip correctly; test updated                    |

---

## Order of Implementation

1. **Step 1** — `SharedValue` enum + free-function conversions (foundation, testable in isolation)
2. **Step 2** — `ObjKind::TaskHandle` + `OpCode::Await` (type/opcode definitions, no behavior change)
3. **Step 3** — `SendableVM` + `fork_for_spawn` + `transfer_closure` (VM forking)
4. **Step 4** — Real `Spawn` handler (core async — can test fire-and-forget immediately)
5. **Step 5** — `Await` handler (completes the spawn-await loop)
6. **Step 6** — Compiler fixes (`Expr::Spawn`, `Stmt::Spawn` upvalue capture, `Expr::Await` opcode)
7. **Step 7** — Remove rejections (enable in `--vm` mode)
8. **Step 8** — Tests (VM async + serialization round-trip)

`cargo test` after each step. Atomic commits.

---

## File Change Summary

| File                  | Changes                                                                                                                |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `src/vm/value.rs`     | `SharedValue` enum, `ObjKind::TaskHandle`, `value_to_shared`/`shared_to_value` free fns, match arm updates (~95 lines) |
| `src/vm/bytecode.rs`  | `OpCode::Await` (1 line)                                                                                               |
| `src/vm/machine.rs`   | `SendableVM`, `fork_for_spawn`, `transfer_closure`, `Spawn` handler, `Await` handler (~175 lines)                      |
| `src/vm/compiler.rs`  | Fix `Expr::Spawn` + `Stmt::Spawn` upvalue capture, split `Expr::Await`, add `ReturnNull` (~35 lines)                   |
| `src/vm/mod.rs`       | 12 async tests (~160 lines)                                                                                            |
| `src/vm/serialize.rs` | Add Spawn/Await to round-trip opcode test (~5 lines)                                                                   |
| `src/main.rs`         | Remove spawn/await rejections (~10 lines removed)                                                                      |

**Total: ~470 lines added/changed across 7 files**

---

## Issues Caught By Review (Audit Trail)

| #   | Issue                                                              | Found in | Severity      |
| --- | ------------------------------------------------------------------ | -------- | ------------- |
| 1   | `VM` not `Send` — raw ptr in `JitEntry`                            | Review 1 | Showstopper   |
| 2   | Cross-GC `GcRef` dangling in closure transfer                      | Review 1 | Showstopper   |
| 3   | Upvalue transfer missing from plan                                 | Review 1 | Showstopper   |
| 4   | `gc.alloc` takes `ObjKind` not `GcObject`                          | Review 1 | Bug           |
| 5   | Missing `method_tables`/`static_methods`/etc in fork               | Review 1 | Bug           |
| 6   | `SharedValue` missing function variants                            | Review 1 | Design gap    |
| 7   | Spawn sub-compiler has no parent context — upvalues dead           | Review 2 | Showstopper   |
| 8   | `fork_for_spawn` overwrites child builtins with Null               | Review 2 | Showstopper   |
| 9   | `call_value` returns `Result<Value>` not `Result<Option<Value>>`   | Review 2 | Won't compile |
| 10  | `Expr::Spawn` missing `ReturnNull` opcode                          | Review 2 | Bug           |
| 11  | Borrow conflict in Await (GC borrow held during `shared_to_value`) | Review 2 | Won't compile |
| 12  | Serialization test doesn't cover new opcodes                       | Review 2 | Test gap      |
| 13  | `value_to_shared` should be free fn, not `impl VM`                 | Review 2 | Borrow risk   |
