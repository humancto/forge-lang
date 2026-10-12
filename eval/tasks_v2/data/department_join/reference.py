import sys

text = sys.stdin.read().strip("\n")
emp_block, dept_block = text.split("\n---\n")


def rows(block):
    lines = [l for l in block.splitlines() if l.strip()]
    head = lines[0].split(",")
    return [dict(zip(head, l.split(","))) for l in lines[1:]]


emps = rows(emp_block)
depts = rows(dept_block)
dept_ids = {d["id"] for d in depts}
out = []
for d in depts:
    staff = [e for e in emps if e["dept"] == d["id"]]
    n = len(staff)
    payroll = sum(int(e["salary"]) for e in staff)
    budget = int(d["budget"])
    hund = (payroll * 200 + n) // (2 * n) if n else 0
    tenths = (payroll * 2000 + budget) // (2 * budget)
    senior = min(staff, key=lambda e: (e["hired"], int(e["id"])))["name"] if staff else "-"
    out.append((tenths, d["name"], "{}: staff={} payroll={} avg={}.{:02d} used={}.{}% senior={}".format(
        d["name"], n, payroll, hund // 100, hund % 100, tenths // 10, tenths % 10, senior)))
for _, _, line in sorted(out, key=lambda t: (-t[0], t[1])):
    print(line)
un = [e["name"] for e in emps if e["dept"] not in dept_ids]
print("unassigned: " + (", ".join(un) if un else "-"))
