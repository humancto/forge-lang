# 6A.1 — Fix interpreter `len()` byte-vs-char parity bug

## Problem

Multiple places in the interpreter use `s.len()` (byte count) instead of `s.chars().count()` (char count). The VM already uses char count. Non-ASCII strings return different values between backends.

## Fix — 4 locations

1. `interpreter/builtins.rs:28` — `len()` builtin function
2. `interpreter/mod.rs:1880` — `.len` method-style on strings
3. `interpreter/mod.rs:2180` — `.len()` method call dispatch
4. `interpreter/builtins.rs:999` — `count("")` edge case uses `s.len() + 1`, should use `s.chars().count() + 1`

All change `s.len()` → `s.chars().count()`.

## Out of scope (tracked separately)

- `index_of` / `last_index_of` return byte offsets via `str::find()` — both backends are consistently wrong. Separate fix needed.

## Test strategy

- Add parity test with multi-byte characters (e.g., `len("café")` = 4, not 5)
- Test emoji: `len("👋")` = 1
- Test `count("café", "")` = 5

## Rollback

4 single-line changes, trivially reversible.
