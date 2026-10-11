emps = [("Ada", "CEO", None), ("Bob", "CTO", "Ada"), ("Cy", "Engineering Lead", "Bob"),
        ("Di", "Engineer", "Cy"), ("Ed", "Engineer", "Cy"), ("Flo", "CFO", "Ada"),
        ("Gus", "Accountant", "Flo")]
by = {e[0]: e for e in emps}
for n in ["Di", "Gus", "Ada"]:
    chain = []
    cur = by[n]
    while cur:
        chain.append(f"{cur[0]} ({cur[1]})")
        cur = by.get(cur[2]) if cur[2] else None
    print(" -> ".join(chain))
reports = {}
for e in emps:
    if e[2]:
        reports[e[2]] = reports.get(e[2], 0) + 1
for k in sorted(reports):
    print(f"{k}: {reports[k]}")
