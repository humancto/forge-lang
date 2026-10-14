import sys
from decimal import ROUND_HALF_UP, Decimal

# Produces the expected output of the Forge modules driven by files/main.fg.
INCH_CM, MILE_KM, LB_KG, GAL_L = 2.54, 1.609344, 0.45359237, 3.785411784
TO_METRIC = {"in": ("cm", lambda v: v * INCH_CM), "mi": ("km", lambda v: v * MILE_KM),
             "lb": ("kg", lambda v: v * LB_KG), "gal": ("l", lambda v: v * GAL_L),
             "F": ("C", lambda v: (v - 32.0) * 5.0 / 9.0)}
TO_IMPERIAL = {"cm": ("in", lambda v: v / INCH_CM), "km": ("mi", lambda v: v / MILE_KM),
               "kg": ("lb", lambda v: v / LB_KG), "l": ("gal", lambda v: v / GAL_L),
               "C": ("F", lambda v: v * 9.0 / 5.0 + 32.0)}


def fixed2(x):
    return str(Decimal(x).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP))


log = []
print("1 in = 2.54 cm, 1 mi = 1.609344 km")
for line in sys.stdin.read().split("\n"):
    parts = line.strip().split(" ")
    if len(parts) != 2:
        continue
    v, u = float(parts[0]), parts[1]
    table = TO_METRIC if u in TO_METRIC else TO_IMPERIAL if u in TO_IMPERIAL else None
    if table is None:
        print("error: unknown imperial unit {0} / unknown metric unit {0}".format(u))
        continue
    to, f = table[u]
    log.append("{}->{}".format(u, to))
    print("{} = {} {}".format(line.strip(), fixed2(f(v)), to))
print("conversions: {}".format(len(log)))
print("history: " + ", ".join(log))
