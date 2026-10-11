import csv
tot, prod = {}, {}
with open("sales.csv") as f:
    for r in csv.DictReader(f):
        u = int(r["units"])
        tot[r["region"]] = tot.get(r["region"], 0) + u
        p = prod.setdefault(r["region"], {})
        p[r["product"]] = p.get(r["product"], 0) + u
lines = []
for reg in sorted(tot):
    best = sorted(prod[reg].items(), key=lambda kv: (-kv[1], kv[0]))[0][0]
    lines.append(f"{reg}: {tot[reg]} units, top product {best}")
lines.append(f"grand total: {sum(tot.values())}")
with open("report.txt", "w") as f:
    f.write("\n".join(lines) + "\n")
print(f"wrote report.txt ({len(tot)} regions)")
