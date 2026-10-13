import json
import sys
from collections import Counter, defaultdict


def dollars(c):
    return "{}.{:02d}".format(c // 100, c % 100)


orders = invalid = 0
rev = Counter()
count = Counter()
skus = Counter()
coupons = Counter()
tiers = Counter()
hours = Counter()
non_ascii = 0
longest = 0
for line in sys.stdin.read().split("\n"):
    if line.strip() == "":
        continue
    try:
        o = json.loads(line)
    except ValueError:
        invalid += 1
        continue
    if "country" not in o["user"] or not o["items"]:
        invalid += 1
        continue
    orders += 1
    sub = 0
    for it in o["items"]:
        sub += it["qty"] * round(it["price"] * 100)
        skus[it["sku"]] += it["qty"]
    if o["coupon"] == "SAVE10":
        sub -= (sub + 5) // 10
    c = o["user"]["country"]
    rev[c] += sub
    count[c] += 1
    coupons[o["coupon"] or "none"] += 1
    tiers[o["user"]["tier"]] += 1
    ts = o["ts"]
    h = int(ts[11:13])
    if not ts.endswith("Z"):
        sign = 1 if ts[-6] == "+" else -1
        h = (h - sign * int(ts[-5:-3])) % 24
    hours[h] += 1
    note = o["note"]
    if any(ord(ch) > 127 for ch in note):
        non_ascii += 1
    longest = max(longest, len(note))

print("orders: {}".format(orders))
print("invalid: {}".format(invalid))
print("by country:")
for c in sorted(rev, key=lambda c: (-rev[c], c)):
    n = count[c]
    print("{} orders={} revenue={} avg={}".format(c, n, dollars(rev[c]), dollars((2 * rev[c] + n) // (2 * n))))
print("top skus:")
for s, q in sorted(skus.items(), key=lambda kv: (-kv[1], kv[0]))[:5]:
    print("{} qty={}".format(s, q))
print("coupons: SAVE10={} FREESHIP={} none={}".format(coupons["SAVE10"], coupons["FREESHIP"], coupons["none"]))
print("tiers: free={} silver={} gold={}".format(tiers["free"], tiers["silver"], tiers["gold"]))
h, n = sorted(hours.items(), key=lambda kv: (-kv[1], kv[0]))[0]
print("busiest utc hour: {:02d} ({} orders)".format(h, n))
print("notes: non-ascii={} longest={}".format(non_ascii, longest))
