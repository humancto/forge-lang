# Plan: 9A.1 — Channel select

## Goal

Add a `select` builtin that waits on multiple channels and returns the first ready value along with the channel index.

## Current state

Channels use `std::sync::mpsc::sync_channel`. `receive(ch)` blocks on a single channel. No way to wait on multiple channels.

## Approach — Builtin function (not new syntax)

The roadmap suggests `select { ch1 -> v1 { }, ch2 -> v2 { } }` syntax, but implementing this as a **builtin function** is simpler and covers the core use case without parser changes:

```forge
let [index, value] = select([ch1, ch2, ch3])
// index = 0, 1, or 2 — which channel was ready
// value = the received value
```

### Implementation

`select(channels)` — takes an array of channels, polls each with try_recv in a loop with short sleeps, returns `[index, value]` when one is ready.

Since Rust's `mpsc` doesn't have a native select, use a polling loop:

1. Loop over channels calling `try_recv()` on each
2. If one succeeds, return `[index, value]`
3. If none ready, sleep briefly (1ms) and retry
4. To avoid starvation, rotate the starting index each iteration

### Files to touch

1. **`src/interpreter/builtins.rs`** — add `select` builtin
2. **`src/interpreter/mod.rs`** — register `select` in builtin list
3. **`src/vm/builtins.rs`** — add VM `select` builtin
4. **`src/vm/compiler.rs`** — add `select` to builtin list (if needed)

### Edge cases

- Empty array → error
- Non-channel in array → error
- All channels closed → return Null
- Single channel → same as receive, but returns [0, value]

## Test strategy

- Unit test: select on channels where one has data ready
- Test: select returns correct index

## Rollback

Revert changes to builtins files.
