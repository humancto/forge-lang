# 6A.2 — Fix `mem::forget(jit)` unbounded memory leak

## Problem

`machine.rs:2038` uses `std::mem::forget(jit)` after extracting a function pointer from a JitCompiler. This prevents the JITModule from being dropped, leaking its memory (Cranelift module allocations, compiled code buffers). Every hot function that gets JIT-compiled leaks hundreds of KB.

The `mem::forget` is there because dropping the JitCompiler would free the compiled code that `jit_cache` holds raw pointers to.

## Fix

Store the `JitCompiler` instances on the VM instead of forgetting them. This way they live as long as the VM and their compiled code remains valid.

1. Add `jit_modules: Vec<JitCompiler>` field to `VM` struct (machine.rs:225 area)
2. Initialize as `Vec::new()` in both `VM::new()` constructors
3. Replace `std::mem::forget(jit)` with `self.jit_modules.push(jit)`
4. In `fork_for_spawn` / `SendableVM`, ensure `jit_modules` is empty (same invariant as `jit_cache`)

## Edge cases

- `fork_for_spawn`: child VM gets empty `jit_modules` (matches existing `jit_cache` pattern)
- `SendableVM` debug_assert: add `jit_modules` emptiness check alongside `jit_cache`
- Drop: when VM drops, `jit_modules` drops, JitCompilers drop, code freed. `jit_cache` pointers become dangling but VM is gone anyway.

## Test strategy

- Existing JIT tests must still pass (fib_30 etc.)
- No new tests needed — this is a resource management fix, not behavior change

## Rollback

3-line change, trivially reversible.
