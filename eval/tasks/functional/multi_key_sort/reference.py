ps = [("Ada", "eng", 120, 36), ("Bob", "ops", 85, 45), ("Cy", "eng", 120, 29), ("Di", "sales", 70, 31),
      ("Ed", "ops", 85, 38), ("Flo", "eng", 95, 52), ("Gus", "sales", 70, 31), ("Hal", "eng", 95, 40)]
print("by department, then salary descending, then name:")
for n, d, s, a in sorted(ps, key=lambda p: (p[1], -p[2], p[0])):
    print(f"  {n} {d} {s}")
print("by age descending, then name:")
for n, d, s, a in sorted(ps, key=lambda p: (-p[3], p[0])):
    print(f"  {n} {a}")
print("top earner per department:")
for d in sorted({p[1] for p in ps}):
    best = sorted((p for p in ps if p[1] == d), key=lambda p: (-p[2], p[0]))[0]
    print(f"  {d}: {best[0]}")
