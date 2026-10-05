#!/usr/bin/env bash
# Forge benchmark runner: one entry point for every wall-clock benchmark,
# with a human table or machine-readable JSON (consumed by
# tools/bench_compare.py and the CI regression gate; see docs/BENCHMARKS.md).
#
# Usage:
#   tools/bench.sh [options] [-- forge flags...]
#
# Options:
#   --json            print JSON on stdout instead of a table
#   --forge PATH      forge binary (default: $FORGE, else target/release/forge)
#   --suite LIST      comma-separated: vm,interp,startup,peers
#                     (default: vm,interp,startup)
#   --filter TEXT     only benchmarks whose id contains TEXT ($BENCH_FILTER)
#   --ids LIST        only these comma-separated benchmark ids (exact match)
#   --runs N          timed runs per benchmark (default 3, $BENCH_RUNS)
#   --warmup N        untimed runs before timing (default 1, $BENCH_WARMUP)
#   --timeout SECS    per-run timeout (default 120, $BENCH_TIMEOUT)
#   --list            print benchmark ids and exit
#
# Suites (ids are <suite>/<program>):
#   vm       benchmarks/vm/*.fg on the default engine (VM + JIT tier-up)
#   interp   benchmarks/interp/*.fg with --interp
#   startup  benchmarks/startup/hello.fg run $STARTUP_BATCH times (default 20)
#            per sample, so process start-up is measured above timer noise
#   peers    the vm workloads in Python / Node / Lua (benchmarks/peers/),
#            for reference only; not in the default set. Interpreters that
#            are not installed are reported as "missing", not as failures.
#
# Flags after `--` replace a suite's own engine flags (vm: none,
# interp: --interp), e.g. `tools/bench.sh --suite vm -- --jit`.
#
# Each sample is the wall-clock time of whole `forge run` processes, so it
# includes start-up. JSON reports every sample plus the median and minimum.
# Exit status: 0 if every benchmark ran, 1 if any failed or timed out,
# 2 on usage errors.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FORGE_BIN="${FORGE:-$ROOT/target/release/forge}"
JSON=0
SUITES="vm,interp,startup"
FILTER="${BENCH_FILTER:-}"
IDS=""
RUNS="${BENCH_RUNS:-3}"
WARMUP="${BENCH_WARMUP:-1}"
TIMEOUT_SECS="${BENCH_TIMEOUT:-120}"
STARTUP_BATCH="${STARTUP_BATCH:-20}"
LIST=0
OVERRIDE_FLAGS=0
EXTRA_FLAGS=()

usage() {
    sed -n '2,37p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --json) JSON=1 ;;
        --forge) FORGE_BIN="${2:?--forge needs a path}"; shift ;;
        --suite) SUITES="${2:?--suite needs a list}"; shift ;;
        --filter) FILTER="${2:?--filter needs text}"; shift ;;
        --ids) IDS="${2:?--ids needs a list}"; shift ;;
        --runs) RUNS="${2:?--runs needs a number}"; shift ;;
        --warmup) WARMUP="${2:?--warmup needs a number}"; shift ;;
        --timeout) TIMEOUT_SECS="${2:?--timeout needs seconds}"; shift ;;
        --list) LIST=1 ;;
        -h|--help) usage ;;
        --) shift; OVERRIDE_FLAGS=1; EXTRA_FLAGS=("$@"); break ;;
        *) echo "bench.sh: unknown option '$1'" >&2; usage ;;
    esac
    shift
done

case "$RUNS$WARMUP$STARTUP_BATCH" in
    *[!0-9]*) echo "bench.sh: --runs, --warmup and STARTUP_BATCH must be integers" >&2; exit 2 ;;
esac
if [ "$RUNS" -lt 1 ]; then
    echo "bench.sh: --runs must be at least 1" >&2
    exit 2
fi

# ---- timing ---------------------------------------------------------------
# Microsecond wall clock: bash 5's $EPOCHREALTIME, else GNU date, else perl.
if [ -n "${EPOCHREALTIME:-}" ]; then
    now_us() { local t="${EPOCHREALTIME/[.,]/}"; echo "$t"; }
elif date +%N | grep -qv N; then
    now_us() { echo $(( $(date +%s%N) / 1000 )); }
else
    now_us() { perl -MTime::HiRes=time -e 'printf "%d\n", time()*1e6'; }
fi
if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD=(timeout "$TIMEOUT_SECS")
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_CMD=(gtimeout "$TIMEOUT_SECS")
else
    TIMEOUT_CMD=()
fi

fmt_s() { # microseconds -> seconds with 6 decimals
    local us=$1
    printf '%d.%06d' $((us / 1000000)) $((us % 1000000))
}

json_str() { # JSON string literal (escapes \, ", control chars)
    local s=${1//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\t'/\\t}
    s=${s//$'\r'/}
    s=${s//$'\n'/\\n}
    s=$(printf '%s' "$s" | tr -d '\000-\010\013\014\016-\037')
    printf '"%s"' "$s"
}

# ---- benchmark list ---------------------------------------------------------
# Each entry: id|file|suite|runner  (runner: forge, an interpreter, or
# "missing:<name>" when a peers interpreter is not installed)
first_cmd() {
    local c
    for c in "$@"; do
        if command -v "$c" >/dev/null 2>&1; then
            echo "$c"
            return
        fi
    done
    echo "missing:$1"
}
BENCHES=()
add_bench() {
    if [ -n "$FILTER" ] && [[ "$1" != *"$FILTER"* ]]; then
        return
    fi
    if [ -n "$IDS" ] && [[ ",$IDS," != *",$1,"* ]]; then
        return
    fi
    BENCHES+=("$1|$2|$3|$4")
}
IFS=',' read -r -a SUITE_LIST <<<"$SUITES"
for suite in "${SUITE_LIST[@]}"; do
    case "$suite" in
        vm|interp|startup)
            for prog in "$ROOT/benchmarks/$suite"/*.fg; do
                [ -e "$prog" ] || continue
                add_bench "$suite/$(basename "$prog" .fg)" "$prog" "$suite" forge
            done
            ;;
        peers)
            for prog in "$ROOT"/benchmarks/peers/*; do
                [ -e "$prog" ] || continue
                file=$(basename "$prog")
                case "$file" in
                    *.py) lang=python; runner=$(first_cmd python3 python) ;;
                    *.js) lang=node; runner=$(first_cmd node) ;;
                    *.lua) lang=lua; runner=$(first_cmd lua lua5.4 lua5.3 luajit) ;;
                    *) continue ;;
                esac
                add_bench "peers/$lang/${file%.*}" "$prog" peers "$runner"
            done
            ;;
        *) echo "bench.sh: unknown suite '$suite' (vm, interp, startup, peers)" >&2; exit 2 ;;
    esac
done

if [ "$LIST" = 1 ]; then
    for b in ${BENCHES[@]+"${BENCHES[@]}"}; do echo "${b%%|*}"; done
    exit 0
fi
if [ ! -x "$FORGE_BIN" ]; then
    echo "bench.sh: forge binary not found: $FORGE_BIN (build with: cargo build --release)" >&2
    exit 2
fi

suite_flags() {
    if [ "$OVERRIDE_FLAGS" = 1 ]; then
        printf '%s\n' ${EXTRA_FLAGS[@]+"${EXTRA_FLAGS[@]}"}
        return
    fi
    case "$1" in
        interp) echo "--interp" ;;
        *) ;;
    esac
}

# Run one sample; sets SAMPLE_US, SAMPLE_OUT, SAMPLE_STATUS (ok|failed|timeout).
run_sample() {
    local prog=$1 suite=$2 runner=$3
    local -a cmd=()
    local f
    if [ "$runner" = forge ]; then
        cmd=("$FORGE_BIN")
        while IFS= read -r f; do
            [ -n "$f" ] && cmd+=("$f")
        done < <(suite_flags "$suite")
        cmd+=(run "$prog")
    else
        cmd=("$runner" "$prog")
    fi
    local reps=1
    [ "$suite" = startup ] && reps=$STARTUP_BATCH
    local start end rc i out=""
    start=$(now_us)
    for ((i = 0; i < reps; i++)); do
        out=$(${TIMEOUT_CMD[@]+"${TIMEOUT_CMD[@]}"} "${cmd[@]}" 2>&1 </dev/null)
        rc=$?
        if [ $rc -ne 0 ]; then
            break
        fi
    done
    end=$(now_us)
    SAMPLE_US=$((end - start))
    SAMPLE_OUT=$(printf '%s\n' "$out" | tail -n 1)
    if [ $rc -eq 0 ]; then
        SAMPLE_STATUS=ok
    elif [ $rc -eq 124 ]; then
        SAMPLE_STATUS=timeout
    else
        SAMPLE_STATUS=failed
    fi
}

median_us() { # median of the arguments (integers)
    local sorted
    sorted=($(printf '%s\n' "$@" | sort -n))
    local n=${#sorted[@]}
    if (( n % 2 )); then
        echo "${sorted[$((n / 2))]}"
    else
        echo $(( (sorted[n / 2 - 1] + sorted[n / 2]) / 2 ))
    fi
}

overall=0
RESULTS=()
if [ "$JSON" = 0 ]; then
    printf '%-24s %10s %10s  %s\n' "benchmark" "median (s)" "min (s)" "output"
fi
for b in ${BENCHES[@]+"${BENCHES[@]}"}; do
    IFS='|' read -r id prog suite runner <<<"$b"
    status=ok
    samples=()
    output=""
    runner_name=forge
    if [ "$runner" != forge ]; then
        runner_name=$(basename "${runner#missing:}")
    fi
    if [[ "$runner" == missing:* ]]; then
        # A peer interpreter that is not installed is reported, not a failure.
        status=missing
        output="${runner#missing:} not installed"
    fi
    for ((w = 0; w < WARMUP; w++)); do
        [ "$status" = ok ] || break
        run_sample "$prog" "$suite" "$runner"
        if [ "$SAMPLE_STATUS" != ok ]; then
            status=$SAMPLE_STATUS
            output=$SAMPLE_OUT
            break
        fi
    done
    if [ "$status" = ok ]; then
        for ((r = 0; r < RUNS; r++)); do
            run_sample "$prog" "$suite" "$runner"
            output=$SAMPLE_OUT
            if [ "$SAMPLE_STATUS" != ok ]; then
                status=$SAMPLE_STATUS
                break
            fi
            samples+=("$SAMPLE_US")
        done
    fi
    if [ "$status" != ok ] && [ "$status" != missing ]; then
        overall=1
    fi
    if [ "$status" = ok ]; then
        med=$(median_us "${samples[@]}")
        min=$(printf '%s\n' "${samples[@]}" | sort -n | head -n 1)
    fi
    if [ "$JSON" = 0 ]; then
        if [ "$status" = ok ]; then
            printf '%-24s %10s %10s  %s\n' "$id" "$(fmt_s "$med")" "$(fmt_s "$min")" "$output"
        else
            printf '%-24s %10s %10s  %s\n' "$id" "-" "-" "$(echo "$status" | tr "[:lower:]" "[:upper:]"): $output"
        fi
        continue
    fi
    times=""
    for s in ${samples[@]+"${samples[@]}"}; do
        times+="${times:+, }$(fmt_s "$s")"
    done
    rec="{\"id\": $(json_str "$id"), \"suite\": $(json_str "$suite"), \"runner\": $(json_str "$runner_name"), \"status\": $(json_str "$status"), \"times_s\": [${times}]"
    if [ "$status" = ok ]; then
        rec+=", \"median_s\": $(fmt_s "$med"), \"min_s\": $(fmt_s "$min")"
    else
        rec+=", \"median_s\": null, \"min_s\": null"
    fi
    if [ "$suite" = startup ]; then
        rec+=", \"batch\": $STARTUP_BATCH"
    fi
    rec+=", \"output\": $(json_str "$output")}"
    RESULTS+=("$rec")
done

if [ "$JSON" = 1 ]; then
    version=$("$FORGE_BIN" version 2>/dev/null | head -n 1)
    printf '{\n'
    printf '  "schema": 1,\n'
    printf '  "forge": %s,\n' "$(json_str "$FORGE_BIN")"
    printf '  "version": %s,\n' "$(json_str "$version")"
    printf '  "host": {"os": %s, "arch": %s, "cpus": %s},\n' \
        "$(json_str "$(uname -s)")" "$(json_str "$(uname -m)")" \
        "$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0)"
    printf '  "runs": %s,\n  "warmup": %s,\n' "$RUNS" "$WARMUP"
    printf '  "benchmarks": [\n'
    n=${#RESULTS[@]}
    for ((i = 0; i < n; i++)); do
        sep=","
        [ $((i + 1)) -eq "$n" ] && sep=""
        printf '    %s%s\n' "${RESULTS[$i]}" "$sep"
    done
    printf '  ]\n}\n'
fi
exit $overall
