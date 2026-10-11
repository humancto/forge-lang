import csv, sys
from decimal import Decimal
tot, cnt = {}, {}
for row in csv.DictReader(sys.stdin):
    c = row["category"]
    tot[c] = tot.get(c, Decimal(0)) + Decimal(row["amount"])
    cnt[c] = cnt.get(c, 0) + 1
print("category,total,count")
for c in sorted(tot):
    print(f"{c},{tot[c]:.2f},{cnt[c]}")
