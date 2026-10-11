import os
rows = []
for n in os.listdir("data"):
    p = os.path.join("data", n)
    if os.path.isfile(p):
        rows.append((n, os.path.getsize(p)))
rows.sort(key=lambda r: (-r[1], r[0]))
for n, s in rows:
    print(n, s)
print(f"files: {len(rows)}, total bytes: {sum(s for _, s in rows)}")
ext = {}
for n, _ in rows:
    e = os.path.splitext(n)[1] or "none"
    ext[e] = ext.get(e, 0) + 1
print("by extension: " + " ".join(f"{k}={v}" for k, v in sorted(ext.items())))
