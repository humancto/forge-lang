#!/usr/bin/env bash
# Time the tree-walking interpreter on its known hot paths.
#
# Usage:
#   tools/bench_interp.sh [forge-binary] [-- extra forge flags...]
#
# Examples:
#   cargo build --release && tools/bench_interp.sh target/release/forge
#   tools/bench_interp.sh target/release/forge -- --vm   # compare engines
#
# Runs every benchmarks/interp/*.fg with `--interp` (unless extra flags are
# given) and prints wall-clock seconds per program. Each program is
# sized so a healthy build finishes in well under a second; anything that
# regresses to quadratic behaviour shows up as seconds or a timeout.
#
# Environment:
#   FORGE          forge binary (overridden by the first argument)
#   BENCH_TIMEOUT  per-program timeout in seconds (default 120)
#   BENCH_RUNS     runs per program; the best time is reported (default 3)
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
if [ ${#FLAGS[@]} -eq 0 ]; then
    FLAGS=(--interp)
fi
TIMEOUT_SECS="${BENCH_TIMEOUT:-120}"
RUNS="${BENCH_RUNS:-3}"

if [ ! -x "$FORGE_BIN" ]; then
    echo "forge binary not found: $FORGE_BIN (build with: cargo build --release)" >&2
    exit 2
fi

printf '%-22s %10s  %s\n' "benchmark" "best (s)" "output"
status=0
for prog in "$ROOT"/benchmarks/interp/*.fg; do
    name="$(basename "$prog" .fg)"
    best=""
    out=""
    for _ in $(seq "$RUNS"); do
        start=$(date +%s.%N)
        if ! out=$(timeout "$TIMEOUT_SECS" "$FORGE_BIN" "${FLAGS[@]}" run "$prog" 2>&1 </dev/null); then
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
