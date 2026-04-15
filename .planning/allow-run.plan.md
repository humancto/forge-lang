# 7D.2 — Add --allow-run permission flag

## Design

Add `--allow-run` CLI flag. When absent, all shell execution builtins return a runtime error.

Affected builtins: sh, shell, sh_lines, sh_json, sh_ok, run_command, which, pipe_to

## Implementation

1. Add `--allow-run` to clap CLI in main.rs
2. Store in a global atomic bool (thread-safe, zero overhead when checked)
3. Add permission check function in a new `src/permissions.rs` module
4. Call check at the top of each shell builtin in both interpreter/builtins.rs and vm/builtins.rs
5. Error message: "Shell execution denied. Use --allow-run to enable sh/shell/run_command."

## Files

- src/main.rs — add CLI arg, set global
- src/permissions.rs — new file, AtomicBool + check fn
- src/interpreter/builtins.rs — add check before shell builtins
- src/vm/builtins.rs — add check before shell builtins

## Test strategy

- Default: sh() returns error
- With --allow-run: sh() works normally
