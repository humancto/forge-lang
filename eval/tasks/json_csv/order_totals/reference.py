import json, sys
data = json.load(sys.stdin)
def money(c): return f"{c // 100}.{c % 100:02d}"
grand, per = 0, {}
for o in data["orders"]:
    sub = sum(i["price_cents"] * i["qty"] for i in o["items"])
    if "coupon" in o:
        sub -= sub * o["coupon"]["percent"] // 100
    grand += sub
    per[o["customer"]] = per.get(o["customer"], 0) + sub
    print(f"{o['id']} {o['customer']}: {money(sub)}")
print(f"grand total: {money(grand)}")
print("best customer: " + max(per, key=lambda k: per[k]))
