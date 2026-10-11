from itertools import combinations
items = [("map", 1, 15), ("compass", 1, 10), ("water", 5, 30), ("sandwich", 3, 25), ("tent", 9, 60),
         ("camera", 4, 35), ("book", 2, 12), ("jacket", 6, 40)]
best = None
for mask in range(1 << len(items)):
    idx = [i for i in range(len(items)) if mask >> i & 1]
    w = sum(items[i][1] for i in idx)
    if w > 15: continue
    v = sum(items[i][2] for i in idx)
    key = (-v, w, idx)
    if best is None or key < best:
        best = key
v, w, idx = -best[0], best[1], best[2]
print(f"best value: {v}")
print(f"weight: {w}")
print("items: " + " ".join(items[i][0] for i in idx))
