#!/usr/bin/env bash
# Time the bytecode VM (and its JIT tier) on its known hot paths.
#
# Usage:
#   tools/bench_vm.sh [forge-binary] [-- extra forge flags...]
#
# Examples:
#   cargo build --release && tools/bench_vm.sh target/release/forge
#   tools/bench_vm.sh target/release/forge -- --jit      # eager JIT
#   tools/bench_vm.sh target/release/forge -- --interp   # compare engines
#
# Runs every benchmarks/vm/*.fg on the default engine (VM with automatic
# JIT tier-up) unless extra flags are given, and prints the best wall-clock
# seconds per program:
#
#   loop          20M-iteration `while` loop in a function called once
#   fib           recursive fib(30) (call-count JIT tier-up)
#   string_build  200k `s = s + "x"` concatenations
#   array_push    100k `a.push(i)` on a local array
#   map_filter    map/filter/reduce over 1M items (closure callbacks)
#
# Report-only: there are no thresholds, because shared machines are noisy.
# A regression to per-instruction overhead or quadratic copying shows up as
# seconds instead of milliseconds.
#
# Environment:
#   FORGE          forge binary (overridden by the first argument)
#   BENCH_TIMEOUT  per-program timeout in seconds (default 120)
#   BENCH_RUNS     runs per program; the best time is reported (default 3)
#   BENCH_FILTER   only run programs whose name contains this substring
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
FLAGS=("$@")
TIMEOUT_SECS="${BENCH_TIMEOUT:-120}"
RUNS="${BENCH_RUNS:-3}"
FILTER="${BENCH_FILTER:-}"

if [ ! -x "$FORGE_BIN" ]; then
    echo "forge binary not found: $FORGE_BIN (build with: cargo build --release)" >&2
    exit 2
fi

printf '%-22s %10s  %s\n' "benchmark" "best (s)" "output"
status=0
for prog in "$ROOT"/benchmarks/vm/*.fg; do
    name="$(basename "$prog" .fg)"
    if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then
        continue
    fi
    best=""
    out=""
    for _ in $(seq "$RUNS"); do
        start=$(date +%s.%N)
        if ! out=$(timeout "$TIMEOUT_SECS" "$FORGE_BIN" ${FLAGS[@]+"${FLAGS[@]}"} run "$prog" 2>&1 </dev/null); then
            out="FAILED/TIMEOUT: $(echo "$out" | tail -1)"
            status=1
            best="-"
            break
        fi
        end=$(date +%s.%N)
        t=$(echo "$end - $start" | bc)
        if [ -z "$best" ] || [ "$(echo "$t < $best" | bc)" = 1 ]; then
            best=$t
        fi
    done
    printf '%-22s %10s  %s\n' "$name" "$best" "$(echo "$out" | tail -1)"
done
exit $status
