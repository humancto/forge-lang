import sys
for line in sys.stdin:
    if "|" not in line:
        continue
    t, rest = line.split("|")
    t = int(t)
    xs = list(map(int, rest.split()))
    seen = {}
    ans = "none"
    for j, x in enumerate(xs):
        if t - x in seen:
            ans = f"{seen[t - x]} {j}"
            break
        seen.setdefault(x, j)
    print(ans)
