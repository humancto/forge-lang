with open("app.log", "a") as f:
    for l in ["INFO job started", "WARN disk at 91%", "INFO job finished"]:
        f.write(l + "\n")
lines = [l for l in open("app.log").read().splitlines() if l]
print(f"lines: {len(lines)}")
c = {}
for l in lines:
    lvl = l.split()[0]
    c[lvl] = c.get(lvl, 0) + 1
for k in sorted(c):
    print(f"{k}={c[k]}")
