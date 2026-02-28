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

run_benchmark() {
    local name="$1"
    local fg_file="$2"
    local py_file="$3"

    echo "----------------------------------------------"
    echo "  Benchmark: $name"
    echo "----------------------------------------------"

    # Run Forge benchmark
    echo ""
    echo "  [Forge]"
    FORGE_START=$(python3 -c "import time; print(time.time())")
    $FORGE run "$fg_file" 2>&1 | sed 's/^/    /'
    FORGE_END=$(python3 -c "import time; print(time.time())")
    FORGE_TIME=$(python3 -c "print(f'{$FORGE_END - $FORGE_START:.3f}s')")
    echo "    Time: $FORGE_TIME"

    # Run Python benchmark
    echo ""
    echo "  [Python]"
    PY_START=$(python3 -c "import time; print(time.time())")
    python3 "$py_file" 2>&1 | sed 's/^/    /'
    PY_END=$(python3 -c "import time; print(time.time())")
    PY_TIME=$(python3 -c "print(f'{$PY_END - $PY_START:.3f}s')")
    echo "    Time: $PY_TIME"

    echo ""

    # Store results for summary
    FORGE_TIMES+=("$FORGE_TIME")
    PY_TIMES+=("$PY_TIME")
    BENCH_NAMES+=("$name")
}

FORGE_TIMES=()
PY_TIMES=()
BENCH_NAMES=()

run_benchmark "Fibonacci (recursive fib(30))" "$BENCH_DIR/bench_fib.fg" "$BENCH_DIR/bench_fib.py"
run_benchmark "Loop (sum 1 to 1,000,000)" "$BENCH_DIR/bench_loop.fg" "$BENCH_DIR/bench_loop.py"
run_benchmark "String concat (10,000 strings)" "$BENCH_DIR/bench_string.fg" "$BENCH_DIR/bench_string.py"
run_benchmark "Array ops (100K map/filter/reduce)" "$BENCH_DIR/bench_array.fg" "$BENCH_DIR/bench_array.py"
run_benchmark "Factorial(20) x 10,000" "$BENCH_DIR/bench_factorial.fg" "$BENCH_DIR/bench_factorial.py"

echo ""
echo "=============================================="
echo "  SUMMARY"
echo "=============================================="
echo ""
printf "%-35s %12s %12s\n" "Benchmark" "Forge" "Python"
printf "%-35s %12s %12s\n" "-----------------------------------" "------------" "------------"
for i in "${!BENCH_NAMES[@]}"; do
    printf "%-35s %12s %12s\n" "${BENCH_NAMES[$i]}" "${FORGE_TIMES[$i]}" "${PY_TIMES[$i]}"
done
echo ""
echo "=============================================="
