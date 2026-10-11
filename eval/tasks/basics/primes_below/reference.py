n = int(input())
ps = [p for p in range(2, n) if all(p % d for d in range(2, int(p ** 0.5) + 1))]
print(" ".join(map(str, ps)))
print(f"count: {len(ps)}")
