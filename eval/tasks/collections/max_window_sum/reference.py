import sys
lines = sys.stdin.read().splitlines()
k = int(lines[0])
xs = list(map(int, lines[1].split()))
best, start = None, 0
cur = sum(xs[:k])
best = cur
for i in range(1, len(xs) - k + 1):
    cur += xs[i + k - 1] - xs[i - 1]
    if cur > best:
        best, start = cur, i
print(f"sum: {best}")
print(f"start: {start}")
print("window: " + " ".join(map(str, xs[start:start + k])))
