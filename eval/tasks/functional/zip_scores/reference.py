import sys
names, scores = sys.stdin.read().splitlines()[:2]
pairs = list(zip(names.split(), map(int, scores.split())))
for i, (n, s) in enumerate(pairs, 1):
    print(f"{i}. {n}: {s}")
best = max(pairs, key=lambda p: p[1])
print(f"best: {best[0]}")
avg = sum(s for _, s in pairs) / len(pairs)
print("above average: " + " ".join(n for n, s in pairs if s > avg))
