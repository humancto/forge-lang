#!/usr/bin/env bash
# Run every examples/*.fg program and fail if any of them fails.
#
# Usage:
#   tools/run_examples.sh [forge-binary] [-- extra forge flags...]
#
# Examples:
#   tools/run_examples.sh                              # target/debug/forge, default engine (VM)
#   tools/run_examples.sh target/release/forge
#   tools/run_examples.sh target/debug/forge -- --interp
#
# Environment:
#   FORGE            forge binary (overridden by the first argument)
#   EXAMPLE_TIMEOUT  per-example timeout in seconds (default 60)
#   EXAMPLES_DIR     directory to scan (default: examples/ next to this script)
#
# Each example runs from the repository root with stdin closed, so a program
# that blocks on input fails fast instead of hanging CI.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXAMPLES_DIR="${EXAMPLES_DIR:-$ROOT/examples}"
TIMEOUT_SECS="${EXAMPLE_TIMEOUT:-60}"

FORGE_BIN="${FORGE:-$ROOT/target/debug/forge}"
if [ $# -gt 0 ] && [ "$1" != "--" ]; then
    FORGE_BIN="$1"
    shift
fi
if [ $# -gt 0 ] && [ "$1" = "--" ]; then
    shift
fi
EXTRA_FLAGS=("$@")

if [ ! -x "$FORGE_BIN" ] && [ -x "$FORGE_BIN.exe" ]; then
    FORGE_BIN="$FORGE_BIN.exe"
fi
if [ ! -x "$FORGE_BIN" ]; then
    echo "error: forge binary not found at '$FORGE_BIN' (build it with 'cargo build' or pass a path)" >&2
    exit 2
fi

# Examples that cannot run unattended in CI. Keep every entry commented with
# the reason; remove an entry as soon as the example becomes self-contained.
SKIP=(
    api.fg                      # starts an HTTP server and never exits
    bench_server.fg             # benchmark HTTP server, never exits
    bench_server_closure.fg     # benchmark HTTP server, never exits
    bench_server_concurrent.fg  # benchmark HTTP server, never exits
    bench_client.fg             # needs bench_server.fg running on localhost
    fetch_demo.fg               # needs outbound internet access
    mysql_demo.fg               # needs a MySQL server
)

# Examples that shell out (sh/shell/run_command) and need --allow-run.
ALLOW_RUN=(
    devops.fg
    showcase.fg
)

contains() {
    local needle="$1"
    shift
    local item
    for item in "$@"; do
        [ "$item" = "$needle" ] && return 0
    done
    return 1
}

if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD=(timeout "$TIMEOUT_SECS")
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_CMD=(gtimeout "$TIMEOUT_SECS")
else
    echo "warning: no 'timeout' command found; examples run without a time limit" >&2
    TIMEOUT_CMD=()
fi

passed=0
failed=0
skipped=0
failures=()

cd "$ROOT" || exit 2
for file in "$EXAMPLES_DIR"/*.fg; do
    name="$(basename "$file")"
    if contains "$name" "${SKIP[@]}"; then
        echo "SKIP  $name"
        skipped=$((skipped + 1))
        continue
    fi

    flags=()
    if contains "$name" "${ALLOW_RUN[@]}"; then
        flags+=(--allow-run)
    fi
    flags+=(${EXTRA_FLAGS[@]+"${EXTRA_FLAGS[@]}"})

    log="$(mktemp)"
    start=$(date +%s)
    ${TIMEOUT_CMD[@]+"${TIMEOUT_CMD[@]}"} "$FORGE_BIN" ${flags[@]+"${flags[@]}"} run "$file" </dev/null >"$log" 2>&1
    status=$?
    elapsed=$(( $(date +%s) - start ))

    if [ $status -eq 0 ]; then
        echo "ok    $name (${elapsed}s)"
        passed=$((passed + 1))
    else
        if [ $status -eq 124 ]; then
            echo "FAIL  $name (timed out after ${TIMEOUT_SECS}s)"
        else
            echo "FAIL  $name (exit $status, ${elapsed}s)"
        fi
        sed 's/^/      | /' "$log" | tail -n 40
        failed=$((failed + 1))
        failures+=("$name")
    fi
    rm -f "$log"
done

echo
echo "examples: $passed passed, $failed failed, $skipped skipped"
if [ $failed -gt 0 ]; then
    echo "failed: ${failures[*]}"
    exit 1
fi
