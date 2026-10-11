import sys
m = [list(map(int, l.split())) for l in sys.stdin.read().splitlines() if l.strip()]
t = [list(r) for r in zip(*m)]
for r in t:
    print(" ".join(map(str, r)))
print("row sums: " + " ".join(str(sum(r)) for r in m))
print("col sums: " + " ".join(str(sum(r)) for r in t))
