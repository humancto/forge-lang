#!/bin/bash
# Forge vs Python Benchmark Runner
# Usage: bash benchmarks/run_benchmarks.sh

set -e

FORGE="./target/release/forge"
BENCH_DIR="$(cd "$(dirname "$0")" && pwd)"

# Check prerequisites
if [ ! -f "$FORGE" ]; then
    echo "Building Forge in release mode..."
    cargo build --release
fi

echo "=============================================="
echo "  Forge vs Python Performance Benchmarks"
echo "=============================================="
echo ""
echo "Forge binary: $FORGE"
echo "Python: $(python3 --version 2>&1)"
echo "Date: $(date)"
echo "Platform: $(uname -ms)"
echo ""
echo "All times are self-reported (internal timing)."
echo ""

run_benchmark() {
    local name="$1"
    local fg_file="$2"
    local py_file="$3"

    echo "----------------------------------------------"
    echo "  Benchmark: $name"
    echo "----------------------------------------------"

    # Run Forge VM (default)
    echo ""
    echo "  [Forge VM]"
    $FORGE run "$fg_file" 2>&1 | sed 's/^/    /'

    # Run Forge interpreter
    echo ""
    echo "  [Forge --interp]"
    $FORGE run --interp "$fg_file" 2>&1 | sed 's/^/    /'

    # Run Python benchmark
    echo ""
    echo "  [Python]"
    python3 "$py_file" 2>&1 | sed 's/^/    /'

    echo ""
}

run_benchmark "Fibonacci (recursive fib(30))" "$BENCH_DIR/bench_fib.fg" "$BENCH_DIR/bench_fib.py"
run_benchmark "Loop (sum 1 to 1,000,000)" "$BENCH_DIR/bench_loop.fg" "$BENCH_DIR/bench_loop.py"
run_benchmark "String concat (10,000 strings)" "$BENCH_DIR/bench_string.fg" "$BENCH_DIR/bench_string.py"
run_benchmark "Array ops (100K map/filter/reduce)" "$BENCH_DIR/bench_array.fg" "$BENCH_DIR/bench_array.py"
run_benchmark "Factorial(20) x 10,000" "$BENCH_DIR/bench_factorial.fg" "$BENCH_DIR/bench_factorial.py"

echo "=============================================="
echo "  All benchmarks use internal timing."
echo "  Compare the 'Time:' lines above."
echo "=============================================="
