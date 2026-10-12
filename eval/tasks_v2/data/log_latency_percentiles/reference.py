import math
import re
import sys

groups = {}
malformed = 0
for line in sys.stdin.read().split("\n"):
    if line.strip() == "":
        continue
    parts = line.split(" ")
    if (len(parts) != 5 or not re.fullmatch(r"\d{3}", parts[3])
            or not re.fullmatch(r"\d+ms", parts[4])):
        malformed += 1
        continue
    _, method, path, status, latency = parts
    path = "/".join(":id" if seg.isdigit() else seg for seg in path.split("/"))
    groups.setdefault(method + " " + path, []).append((int(status), int(latency[:-2])))


def nearest_rank(values, p):
    return values[math.ceil(p * len(values) / 100) - 1]


rows = sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0]))
for endpoint, reqs in rows:
    lat = sorted(l for _, l in reqs)
    n = len(lat)
    errors = sum(1 for s, _ in reqs if s >= 500)
    tenths = (errors * 2000 + n) // (2 * n)  # percentage * 10, halves up
    print("{} count={} p50={}ms p95={}ms max={}ms errors={}.{}%".format(
        endpoint, n, nearest_rank(lat, 50), nearest_rank(lat, 95), lat[-1],
        tenths // 10, tenths % 10))
print("malformed: {}".format(malformed))
