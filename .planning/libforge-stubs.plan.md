# Plan: M5 Foundation — Static library build + AOT entry point

## Goal

Enable `forge build --aot app.fg` to produce a standalone binary that doesn't require the `forge` CLI at runtime. Currently `--aot` embeds bytecode in a C wrapper that exec's `forge run`. The goal is to link against `libforge.a` directly.

## Current State (verified)

- `src/native.rs` creates C launchers that exec `forge` at runtime (not standalone)
- `src/vm/jit/runtime.rs` has 25+ extern "C" bridge functions already (rt_print, rt_call_native, rt_string_concat, rt_array_new, rt_object_new, etc.)
- The JIT uses Cranelift for in-process compilation with these bridge functions
- `Cargo.toml` builds a binary only — no `[lib]` target exists

## Approach

### Step 1: Add `[lib]` target to Cargo.toml

Add a library target so the crate can be compiled as both binary and static library:

```toml
[lib]
name = "forge_lang"
path = "src/lib.rs"
crate-type = ["staticlib", "rlib"]
```

Create `src/lib.rs` that re-exports the modules needed by AOT binaries:

- `vm::jit::runtime` (bridge functions)
- `vm::machine::VM` (for init/cleanup)
- `vm::gc::Gc`
- `vm::compiler` (bytecode compilation)

### Step 2: AOT entry point function

Add `pub extern "C" fn forge_run_bytecode(bytecode: *const u8, len: usize) -> i32` to `src/lib.rs`:

- Deserializes bytecode into a Chunk
- Creates a VM
- Runs the bytecode
- Returns 0 on success, 1 on error

### Step 3: Update `build_native_aot` in `src/native.rs`

Change the AOT launcher from exec'ing `forge run` to:

- Linking against `libforge.a` (found via `FORGE_LIB_DIR` or next to the forge binary)
- The C wrapper calls `forge_run_bytecode(embedded_bytecode, len)` instead of `execvp("forge", ...)`

### Step 4: Tests

- `cargo build` still works (binary target)
- `cargo test` still passes
- New integration test: compile a simple program with `--aot`, verify the binary runs without `forge` on PATH

## Risk Mitigation

- Binary target unchanged — `src/main.rs` stays as-is
- Library target is additive
- Existing `--aot` behavior preserved as fallback if `libforge.a` not found

## Rollback

Remove `[lib]` from Cargo.toml, delete `src/lib.rs`, revert `native.rs` changes.
