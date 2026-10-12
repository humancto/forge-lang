import math
import re
import sys
from decimal import ROUND_HALF_UP, Decimal

PI = 3.141592653589793


def fmt(x):
    return str(Decimal(x).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP))


shapes = []
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    if not line.strip():
        continue
    p = line.split()
    kind, raw = p[0], p[1:]
    arity = {"circle": 1, "rect": 2, "triangle": 3}.get(kind)
    if (arity is None or len(raw) != arity
            or not all(re.fullmatch(r"\d+(\.\d+)?", v) and float(v) > 0 for v in raw)):
        print("line {}: invalid".format(n))
        continue
    v = [float(x) for x in raw]
    if kind == "circle":
        area, per, label = PI * v[0] * v[0], 2 * PI * v[0], "circle r=" + raw[0]
    elif kind == "rect":
        area, per, label = v[0] * v[1], 2 * (v[0] + v[1]), "rect {}x{}".format(*raw)
    else:
        a, b, c = v
        if not (a < b + c and b < a + c and c < a + b):
            print("line {}: invalid".format(n))
            continue
        s = (a + b + c) / 2
        area, per, label = math.sqrt(s * (s - a) * (s - b) * (s - c)), a + b + c, \
            "triangle {}-{}-{}".format(*raw)
    shapes.append((area, per, label))

for area, per, label in sorted(shapes, key=lambda s: -s[0]):
    print("{}: area={} perimeter={}".format(label, fmt(area), fmt(per)))
print("total area={}".format(fmt(sum(s[0] for s in shapes))))
best = shapes[0]
for s in shapes:
    if s[1] > best[1]:
        best = s
print("largest perimeter: " + best[2])
