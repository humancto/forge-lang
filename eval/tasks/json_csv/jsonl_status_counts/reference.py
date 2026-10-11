import json, sys
stats = {}
slow = None
for line in sys.stdin:
    if not line.strip():
        continue
    r = json.loads(line)
    s = stats.setdefault(r["path"], [0, 0, 0])
    s[0] += 1
    s[1] += r["status"] >= 500
    s[2] += r["ms"]
    if slow is None or r["ms"] > slow["ms"]:
        slow = r
for p in sorted(stats):
    c, e, t = stats[p]
    print(f"{p}: {c} requests, {e} errors, avg {t // c}ms")
print(f"slowest: {slow['path']} {slow['ms']}ms")
