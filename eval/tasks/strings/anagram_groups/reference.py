import sys
groups = {}
for w in sys.stdin.read().split():
    groups.setdefault("".join(sorted(w)), []).append(w)
lines = sorted(" ".join(sorted(g)) for g in groups.values())
for l in lines:
    print(l)
print(f"groups: {len(lines)}")
