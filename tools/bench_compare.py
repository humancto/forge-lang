#!/usr/bin/env python3
"""A/B benchmark comparison and regression gate (docs/BENCHMARKS.md).

Runs tools/bench.sh --json against a BASE and a HEAD forge binary in
interleaved rounds on the same machine (A B, B A, A B, ...), so slow drift
of the machine affects both sides equally. It takes the median and the best
of all samples per side, and fails when a benchmark's HEAD median *and* best
sample are both more than --threshold slower than BASE.

  tools/bench_compare.py --base /tmp/forge-base --head target/release/forge \\
      --rounds 5 --threshold 0.15 --summary "$GITHUB_STEP_SUMMARY"

Both binaries run the *same* benchmark programs (this checkout's
benchmarks/), so a difference is the engine, not the workload. A benchmark
that only HEAD can run is reported as new; one that HEAD can no longer run
is a failure.

Noise control: a slowdown counts only if it exceeds both the relative
threshold and --min-delta seconds (absolute), and the
run-to-run spread is reported so a reviewer can judge borderline results.

Exit status: 0 ok, 1 regression or HEAD failure (unless --allow-regression),
2 usage/infrastructure error. Standard library only.
"""

import argparse
import json
import os
import statistics
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BENCH = os.path.join(ROOT, "tools", "bench.sh")


def run_suite(forge, args, label, ids=None):
    cmd = ["bash", BENCH, "--json", "--forge", forge, "--runs", str(args.runs),
           "--warmup", str(args.warmup), "--suite", args.suite]
    if args.filter:
        cmd += ["--filter", args.filter]
    if ids:
        cmd += ["--ids", ",".join(ids)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode == 2 or not proc.stdout.strip():
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"bench.sh failed for {label} ({forge}): exit {proc.returncode}")
    try:
        return json.loads(proc.stdout)
    except ValueError as e:
        sys.stderr.write(proc.stdout)
        raise SystemExit(f"bench.sh produced invalid JSON for {label}: {e}")


def collect(rounds):
    """{id: {"times": [...], "status": worst status, "output": last}}"""
    out = {}
    for result in rounds:
        for b in result["benchmarks"]:
            slot = out.setdefault(b["id"], {"times": [], "status": "ok", "output": ""})
            slot["times"].extend(b["times_s"])
            slot["output"] = b.get("output", "")
            if b["status"] != "ok":
                slot["status"] = b["status"]
    return out


def spread(times):
    """Relative interquartile-ish spread: (max - min) / median."""
    if len(times) < 2:
        return 0.0
    med = statistics.median(times)
    return (max(times) - min(times)) / med if med else 0.0


def compare(base, head, threshold, min_delta):
    rows = []
    for bid in sorted(set(base) | set(head)):
        b, h = base.get(bid), head.get(bid)
        row = {"id": bid, "base": None, "head": None, "change": None, "verdict": ""}
        if h is None:
            row["verdict"] = "removed"
        elif h["status"] == "missing":
            row["verdict"] = "skipped (not installed)"
        elif h["status"] != "ok":
            row["verdict"] = f"HEAD {h['status']}"
            row["fail"] = True
        elif b is None or b["status"] != "ok":
            row["head"] = statistics.median(h["times"])
            row["verdict"] = "new"
        else:
            bm, hm = statistics.median(b["times"]), statistics.median(h["times"])
            bmin, hmin = min(b["times"]), min(h["times"])
            change = (hm - bm) / bm if bm else 0.0
            min_change = (hmin - bmin) / bmin if bmin else 0.0
            row.update(base=bm, head=hm, change=change, min_change=min_change,
                       spread=max(spread(b["times"]), spread(h["times"])))
            # Contention only ever adds time, so a real slowdown moves the
            # best sample as well as the median; requiring both rejects
            # medians dragged up by a noisy neighbour.
            if (change > threshold and min_change > threshold
                    and (hm - bm) > min_delta):
                row["verdict"] = "REGRESSION"
                row["fail"] = True
            elif (change < -threshold and min_change < -threshold
                    and (bm - hm) > min_delta):
                row["verdict"] = "faster"
            else:
                row["verdict"] = "ok"
            if b["output"] != h["output"]:
                row["verdict"] += " (output differs)"
        rows.append(row)
    return rows


def fmt_s(v):
    return "-" if v is None else f"{v:.4f}"


def fmt_pct(v):
    return "-" if v is None else f"{v * 100:+.1f}%"


def markdown(rows, args, base_meta, head_meta, failed, overridden):
    lines = ["## Benchmark comparison (HEAD vs BASE)", ""]
    if failed and overridden:
        lines.append(f"**Regressions found, allowed by the `{args.override_label}` label.**")
    elif failed:
        lines.append(f"**Regression gate failed:** a benchmark is more than "
                     f"{args.threshold * 100:.0f}% slower (and > {args.min_delta * 1000:.0f} ms), "
                     f"or HEAD failed to run it. If the slowdown is intended, add the "
                     f"`{args.override_label}` label to the pull request.")
    else:
        lines.append(f"No benchmark regressed by more than {args.threshold * 100:.0f}%.")
    lines += [
        "",
        f"{args.rounds} interleaved rounds x {args.runs} run(s) per side, plus "
        f"{args.confirm_rounds} confirmation rounds for flagged benchmarks. A regression needs "
        f"both the median and the best sample more than {args.threshold * 100:.0f}% slower. "
        f"Base `{base_meta.get('version', '?')}`, head `{head_meta.get('version', '?')}`, "
        f"{head_meta.get('host', {}).get('cpus', '?')} CPUs.",
        "",
        "| benchmark | base median (s) | head median (s) | median change | best change "
        "| spread | verdict |",
        "|---|---:|---:|---:|---:|---:|---|",
    ]
    for r in rows:
        verdict = f"**{r['verdict']}**" if r.get("fail") else r["verdict"]
        sp = "-" if r.get("spread") is None else f"{r['spread'] * 100:.0f}%"
        lines.append(f"| `{r['id']}` | {fmt_s(r['base'])} | {fmt_s(r['head'])} | "
                     f"{fmt_pct(r['change'])} | {fmt_pct(r.get('min_change'))} | {sp} | {verdict} |")
    lines += ["", "Spread = (max - min) / median across samples of the noisier side; "
                  "a change smaller than the spread is likely noise.", ""]
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="A/B benchmark regression gate")
    ap.add_argument("--base", required=True, help="baseline forge binary")
    ap.add_argument("--head", required=True, help="candidate forge binary")
    ap.add_argument("--rounds", type=int, default=5, help="interleaved rounds (default 5)")
    ap.add_argument("--runs", type=int, default=1, help="runs per side per round (default 1)")
    ap.add_argument("--warmup", type=int, default=1, help="warm-up runs per round (default 1)")
    ap.add_argument("--threshold", type=float, default=0.15,
                    help="relative slowdown that fails the gate (default 0.15)")
    ap.add_argument("--min-delta", type=float, default=0.010,
                    help="ignore slowdowns smaller than this many seconds (default 0.010)")
    ap.add_argument("--confirm-rounds", type=int, default=5,
                    help="extra interleaved rounds for flagged benchmarks before failing "
                         "(default 5; 0 disables)")
    ap.add_argument("--suite", default="vm,interp,startup")
    ap.add_argument("--filter", default="")
    ap.add_argument("--summary", help="append the Markdown report to this file "
                                      "(e.g. $GITHUB_STEP_SUMMARY)")
    ap.add_argument("--json-out", help="write the raw comparison as JSON")
    ap.add_argument("--allow-regression", action="store_true",
                    help="report regressions but exit 0 (override label)")
    ap.add_argument("--override-label", default="perf-regression-ok")
    args = ap.parse_args(argv)
    if args.rounds < 1 or args.runs < 1 or args.confirm_rounds < 0:
        ap.error("--rounds and --runs must be >= 1, --confirm-rounds >= 0")
    for path in (args.base, args.head):
        if not os.access(path, os.X_OK):
            ap.error(f"not an executable: {path}")

    base_rounds, head_rounds = [], []

    def interleave(rounds, ids=None, tag="round"):
        for i in range(rounds):
            order = [("base", args.base), ("head", args.head)]
            if i % 2:
                order.reverse()
            for label, forge in order:
                print(f"{tag} {i + 1}/{rounds}: {label}", file=sys.stderr, flush=True)
                result = run_suite(forge, args, label, ids)
                (base_rounds if label == "base" else head_rounds).append(result)

    interleave(args.rounds)
    rows = compare(collect(base_rounds), collect(head_rounds), args.threshold, args.min_delta)

    # Confirmation: re-measure only the flagged benchmarks and decide on all
    # samples. A one-off burst of runner noise rarely survives a second pass.
    flagged = [r["id"] for r in rows if r["verdict"].startswith("REGRESSION")]
    if flagged and args.confirm_rounds:
        print(f"confirming {', '.join(flagged)}", file=sys.stderr, flush=True)
        interleave(args.confirm_rounds, flagged, tag="confirm")
        rows = compare(collect(base_rounds), collect(head_rounds), args.threshold,
                       args.min_delta)
        for r in rows:
            if r["id"] in flagged:
                r["confirmed"] = True
                if r.get("fail"):
                    r["verdict"] += " (confirmed)"
                else:
                    r["verdict"] += " (noise: not confirmed)"
    failed = any(r.get("fail") for r in rows)
    report = markdown(rows, args, base_rounds[0], head_rounds[0], failed, args.allow_regression)
    print(report)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as f:
            f.write(report + "\n")
    if args.json_out:
        with open(args.json_out, "w", encoding="utf-8") as f:
            json.dump({"threshold": args.threshold, "min_delta": args.min_delta,
                       "rounds": args.rounds, "rows": rows,
                       "base": base_rounds, "head": head_rounds}, f, indent=2)
    if failed and not args.allow_regression:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
