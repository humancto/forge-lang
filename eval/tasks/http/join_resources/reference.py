import json, sys, urllib.request
base = sys.stdin.readline().strip()
def get(p):
    with urllib.request.urlopen(base + p) as r:
        return json.loads(r.read())
users, orders = get("/users.json"), get("/orders.json")
by_id = {u["id"]: u for u in users}
agg, unknown = {}, set()
for o in orders:
    if o["user_id"] not in by_id:
        unknown.add(o["user_id"])
        continue
    a = agg.setdefault(o["user_id"], [0, 0])
    a[0] += 1
    a[1] += o["total_cents"]
for uid, (n, c) in sorted(agg.items(), key=lambda kv: -kv[1][1]):
    print(f"{by_id[uid]['name']}: {n} orders, {c // 100}.{c % 100:02d}")
print("no orders: " + ", ".join(sorted(u["name"] for u in users if u["id"] not in agg)))
print("unknown user ids: " + (",".join(map(str, sorted(unknown))) or "none"))
