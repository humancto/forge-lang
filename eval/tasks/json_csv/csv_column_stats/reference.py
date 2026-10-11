import csv, sys
from decimal import Decimal, ROUND_HALF_UP
rows = [r for r in csv.DictReader(sys.stdin) if r["temp"].strip()]
temps = [int(r["temp"]) for r in rows]
mean = Decimal(sum(temps)) / Decimal(len(temps))
mean = mean.quantize(Decimal("0.1"), rounding=ROUND_HALF_UP)
print(f"count={len(temps)} min={min(temps)} max={max(temps)} mean={mean}")
print("coldest: " + rows[temps.index(min(temps))]["station"])
print("warmest: " + rows[temps.index(max(temps))]["station"])
