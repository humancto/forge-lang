import sys
from collections import Counter
xs = list(map(int, sys.stdin.read().split()))
c = Counter(xs)
best = max(c.values())
mode = min(v for v, n in c.items() if n == best)
s = sorted(xs)
n = len(s)
if n % 2:
    med = str(s[n // 2])
else:
    tot = s[n // 2 - 1] + s[n // 2]
    med = str(tot // 2) if tot % 2 == 0 else f"{tot / 2:.1f}"
print(f"count: {n}")
print(f"mode: {mode}")
print(f"median: {med}")
print(f"min: {s[0]}")
print(f"max: {s[-1]}")
