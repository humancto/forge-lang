import sys
g = {}
for line in sys.stdin:
    p = line.split()
    if len(p) == 2:
        g.setdefault(p[1], []).append(p[0])
for k in sorted(g):
    print(f"{k}: {', '.join(sorted(g[k]))} ({len(g[k])})")
