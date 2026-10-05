#!/usr/bin/env bash
# Time the bytecode VM (and its JIT tier) on its known hot paths.
# Thin wrapper over tools/bench.sh (the single benchmark runner).
#
# Usage:
#   tools/bench_vm.sh [forge-binary] [-- extra forge flags...]
#
# Examples:
#   cargo build --release && tools/bench_vm.sh target/release/forge
#   tools/bench_vm.sh target/release/forge -- --jit      # eager JIT
#   tools/bench_vm.sh target/release/forge -- --interp   # compare engines
#
#   loop          20M-iteration `while` loop in a function called once
#   fib           recursive fib(30) (call-count JIT tier-up)
#   string_build  200k `s = s + "x"` concatenations
#   array_push    100k `a.push(i)` on a local array
#   map_filter    map/filter/reduce over 1M items (closure callbacks)
#
# Environment: FORGE, BENCH_TIMEOUT, BENCH_RUNS, BENCH_FILTER (see bench.sh).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FORGE_BIN="${FORGE:-$ROOT/target/release/forge}"
if [ $# -gt 0 ] && [ "$1" != "--" ]; then
    FORGE_BIN="$1"
    shift
fi
if [ $# -gt 0 ] && [ "$1" = "--" ]; then
    shift
fi
if [ $# -gt 0 ]; then
    exec "$ROOT/tools/bench.sh" --suite vm --forge "$FORGE_BIN" -- "$@"
fi
exec "$ROOT/tools/bench.sh" --suite vm --forge "$FORGE_BIN"
