import sys
def lev(a, b):
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]
for line in sys.stdin.read().splitlines():
    if "," not in line:
        continue
    a, b = line.split(",", 1)
    show = lambda s: s if s else '""'
    print(f"{show(a)} -> {show(b)}: {lev(a, b)}")
