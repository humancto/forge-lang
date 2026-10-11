import sys
seen, out = set(), []
toks = sys.stdin.read().split()
for t in toks:
    if t not in seen:
        seen.add(t)
        out.append(t)
print(" ".join(out))
print(f"removed: {len(toks) - len(out)}")
