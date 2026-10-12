import json
import sys

emps = json.loads(sys.stdin.read())
teams = {}
for e in emps:
    t = teams.setdefault(e["team"], {"team": e["team"], "people": 0, "payroll": 0})
    t["people"] += 1
    t["payroll"] += e["salary"]
rows = sorted(teams.values(), key=lambda r: (-r["payroll"], r["team"]))


def quote(cell):
    s = str(cell)
    if "," in s or '"' in s:
        return '"' + s.replace('"', '""') + '"'
    return s


head = list(rows[0])
print("## CSV")
print(",".join(head))
for r in rows:
    print(",".join(quote(r[k]) for k in head))
print("## Markdown")
print("| " + " | ".join(head) + " |")
print("|" + "|".join("---" for _ in head) + "|")
for r in rows:
    print("| " + " | ".join(str(r[k]).replace("|", "\\|") for k in head) + " |")
