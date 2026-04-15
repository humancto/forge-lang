# Plan: Tiered compilation (interpret → profile → JIT)

## Goal

Enable automatic tiered compilation: functions start interpreted, get profiled for hotness, and are JIT-compiled when hot — all without user flags.

## Current State

- Profiler exists with `is_hot()` (HOT_THRESHOLD = 100 calls), timing, reporting
- Auto-JIT code exists in Call handler: checks `profiler.is_hot()`, compiles, caches
- JIT dispatch code exists: routes cached functions to native code
- **Problem:** Profiler is disabled by default (`VM::new()` → `Profiler::new(false)`). Auto-JIT never triggers in normal `--vm` mode.
- `--profile` flag enables profiler + timing overhead, but that's for user-facing reports, not auto-JIT

## Approach

### Option A: Always enable the full profiler

Simple but wasteful — `enter_function` does string allocation + HashMap lookup + Instant::now() on every call even when not `--profile`.

### Option B: Lightweight call counter separate from profiler (CHOSEN)

Add a simple `HashMap<String, u32>` call counter that's always active when `jit` feature is enabled. No timing overhead. The full profiler stays disabled by default for `--profile` reporting.

Actually, even simpler: just change `profiler.enter_function()` to skip timing when not in profile mode but still count calls. The profiler already guards timing with `if !self.enabled { return; }`. We can split this: always count calls (when jit feature), but only track timing when enabled.

### Simplest correct approach

1. Add `jit_counting: bool` to Profiler — when true, count calls even when profiler timing is disabled
2. `VM::new()` sets `jit_counting = cfg!(feature = "jit")`
3. `enter_function()` increments call count if `jit_counting || enabled`, but only pushes timing if `enabled`
4. `is_hot()` works regardless of `enabled` (it just reads the counter)

Actually even simpler: the profiler's `enter_function` does 3 things: (1) increment counter, (2) push call stack for timing. We can separate these. But the cleanest approach is:

**Just enable counting unconditionally in enter_function. Guard only the timing push.**

This means `enter_function` always counts, but the Instant::now() + call_stack push only happens when `enabled`. The HashMap lookup for incrementing is cheap.

## Changes

### `src/vm/profiler.rs`

- `enter_function`: Always increment call count. Only push to call_stack when `enabled`.
- `exit_function`: Unchanged (already guards on `enabled`).

### `src/vm/machine.rs`

- No changes needed — `profiler.enter_function()` is already called unconditionally for named functions in the Call handler.

Wait, let me verify... Actually `enter_function` returns early when `!self.enabled`. Let me re-check.

Looking at the code: `enter_function` does `if !self.enabled { return; }` at line 43. So when profiler is disabled, NO counting happens. I need to change this.

### Actual changes:

1. **`src/vm/profiler.rs`**: Remove the early return from `enter_function` when disabled. Instead, always count calls, but only track timing when enabled.

2. That's it. The auto-JIT path already checks `profiler.is_hot()` which reads call_count. Once counting works, auto-JIT triggers automatically.

## Edge Cases

- `exit_function()` also early-returns when disabled. That's fine — we're not pushing to call_stack when disabled, so there's nothing to pop.
- `enter_function` allocates a String for the HashMap key on every call. This is the main overhead. For hot functions, the key already exists (just incrementing), so the allocation is wasted. Could use `entry()` API which avoids allocation when key exists... but it already uses `entry()` which requires an owned String. Acceptable overhead.
- String allocation per call is the real cost. Could optimize later with interned keys, but for now it's fine — it's a HashMap lookup + u32 increment, no timing.

## Test Strategy

- Existing profiler tests should still pass (they use `Profiler::new(true)`)
- Add test: `Profiler::new(false)` still counts calls and `is_hot()` works
- Existing JIT tests exercise auto-JIT indirectly (run_jit_function calls execute which triggers Call)
- Integration test: a function called 100+ times gets JIT-compiled automatically

## Rollback

Revert the `enter_function` change — put back the early return.
