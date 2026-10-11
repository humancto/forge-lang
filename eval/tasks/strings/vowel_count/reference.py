words = input().split()
counts = [(w, sum(c in "aeiouAEIOU" for c in w)) for w in words]
for w, n in counts:
    print(f"{w}: {n}")
print(f"total: {sum(n for _, n in counts)}")
best = max(counts, key=lambda wc: wc[1])
print(f"max: {best[0]}")
