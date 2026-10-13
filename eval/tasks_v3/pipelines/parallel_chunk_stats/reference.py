import sys

lines = [l for l in sys.stdin.read().split("\n") if l.strip()]
chunk = (len(lines) + 7) // 8
parts = [lines[i * chunk:(i + 1) * chunk] for i in range(8)]


def stats(part):
    out = {"n": 0, "bad": 0, "sum": 0, "min": None, "max": None, "sensors": {}}
    for line in part:
        sensor, value = line.split(",")
        try:
            v = int(value)
        except ValueError:
            out["bad"] += 1
            continue
        out["n"] += 1
        out["sum"] += v
        out["min"] = v if out["min"] is None else min(out["min"], v)
        out["max"] = v if out["max"] is None else max(out["max"], v)
        s = out["sensors"].setdefault(sensor, [0, 0])
        s[0] += 1
        s[1] += v
    return out


results = [stats(p) for p in parts]
for i, r in enumerate(results):
    print("chunk {}: n={} bad={} sum={} min={} max={}".format(i + 1, r["n"], r["bad"], r["sum"], r["min"], r["max"]))
total = {}
for r in results:
    for s, (n, sm) in r["sensors"].items():
        t = total.setdefault(s, [0, 0])
        t[0] += n
        t[1] += sm
n = sum(r["n"] for r in results)
print("all: n={} bad={} sum={} min={} max={}".format(
    n, sum(r["bad"] for r in results), sum(r["sum"] for r in results),
    min(r["min"] for r in results), max(r["max"] for r in results)))
ranked = sorted(total.items(), key=lambda kv: (-kv[1][1], kv[0]))
for s, (cnt, sm) in ranked[:3] + ranked[-3:]:
    print("{} n={} sum={}".format(s, cnt, sm))
