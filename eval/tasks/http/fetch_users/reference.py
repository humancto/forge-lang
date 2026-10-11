import json, sys, urllib.request
base = sys.stdin.readline().strip()
with urllib.request.urlopen(base + "/users.json") as r:
    users = json.loads(r.read())
old = sorted((u for u in users if u["age"] > 30), key=lambda u: u["name"])
for u in old:
    print(f"{u['name']} ({u['team']})")
print("teams: " + ",".join(sorted({u["team"] for u in old})))
