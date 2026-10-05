#!/usr/bin/env bash
# Time the tree-walking interpreter on its known hot paths.
# Thin wrapper over tools/bench.sh (the single benchmark runner).
#
# Usage:
#   tools/bench_interp.sh [forge-binary] [-- extra forge flags...]
#
# Examples:
#   cargo build --release && tools/bench_interp.sh target/release/forge
#   tools/bench_interp.sh target/release/forge -- --vm   # compare engines
#
# Runs every benchmarks/interp/*.fg with `--interp` (extra flags replace
# it). Each program is sized so a healthy build finishes in well under a
# second; anything that regresses to quadratic behaviour shows up as
# seconds or a timeout.
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
    exec "$ROOT/tools/bench.sh" --suite interp --forge "$FORGE_BIN" -- "$@"
fi
exec "$ROOT/tools/bench.sh" --suite interp --forge "$FORGE_BIN"
