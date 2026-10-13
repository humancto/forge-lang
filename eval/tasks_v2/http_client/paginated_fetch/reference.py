import json
import sys
import urllib.request

base = sys.stdin.read().strip()
items = []
pages = 0
nxt = "start.json"
while nxt:
    with urllib.request.urlopen(base + "/" + nxt) as r:
        doc = json.loads(r.read())
    pages += 1
    items += doc["items"]
    nxt = doc["next"]

print("pages: {}".format(pages))
print("items: {}".format(len(items)))
cur = {}
for it in items:
    c = cur.setdefault(it["currency"], [0, 0])
    c[0] += round(it["amount"] * 100)
    c[1] += 1
for k in sorted(cur):
    cents, n = cur[k]
    print("{} {}.{:02d} ({} items)".format(k, cents // 100, cents % 100, n))
owners = {}
for it in items:
    owners[it["owner"]] = owners.get(it["owner"], 0) + 1
for o, n in sorted(owners.items(), key=lambda kv: (-kv[1], kv[0])):
    print("{}: {}".format(o, n))
