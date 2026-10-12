import json
import sys

orders = json.loads(sys.stdin.read())["orders"]
customers = {}
totals = {"customers": 0, "paid_orders": 0, "revenue_cents": 0, "refunded_orders": 0}
for o in orders:
    cid = o["customer"]["id"]
    c = customers.setdefault(cid, {"name": o["customer"]["name"], "paid": 0, "spent": 0,
                                   "skus": set(), "best": None, "best_total": -1})
    total = sum(i["qty"] * i["unit_cents"] for i in o["items"])
    if "coupon" in o:
        total = total * (100 - o["coupon"]["percent"]) // 100
    if o["status"] == "refunded":
        totals["refunded_orders"] += 1
    if o["status"] != "paid":
        continue
    c["paid"] += 1
    c["spent"] += total
    c["skus"].update(i["sku"] for i in o["items"])
    if total > c["best_total"]:
        c["best"], c["best_total"] = o["id"], total
    totals["paid_orders"] += 1
    totals["revenue_cents"] += total
totals["customers"] = len(customers)

for cid, c in sorted(customers.items(), key=lambda kv: (-kv[1]["spent"], kv[0])):
    out = {"customer": cid, "name": c["name"], "paid_orders": c["paid"],
           "spent_cents": c["spent"], "skus": sorted(c["skus"]), "largest_order": c["best"]}
    print(json.dumps(out, separators=(",", ":"), ensure_ascii=False))
print(json.dumps(totals, separators=(",", ":")))
