import csv
import io
import re
import sys
from decimal import ROUND_HALF_UP, Decimal

# Produces the expected output of the csvlite package driven by files/main.fg.
NUM = re.compile(r"-?[0-9]+(\.[0-9]+)?")


def fixed2(x):
    return str(Decimal(x).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP))


rows = [r for r in csv.reader(io.StringIO(sys.stdin.read())) if r]
if not rows:
    print("error: empty input")
    sys.exit(0)
headers = [h.strip() for h in rows[0]]
data = rows[1:]
for n, r in enumerate(data, 1):
    if len(r) != len(headers):
        print("error: row {} has {} fields, expected {}".format(n, len(r), len(headers)))
        sys.exit(0)
widths = [max([len(h)] + [len(r[i]) for r in data]) for i, h in enumerate(headers)]
print(" | ".join(h.ljust(w) for h, w in zip(headers, widths)).rstrip())
print("-+-".join("-" * w for w in widths))
for r in data:
    cells = [v.rjust(w) if NUM.fullmatch(v) else v.ljust(w) for v, w in zip(r, widths)]
    print(" | ".join(cells).rstrip())
print("rows: {}".format(len(data)))
for i, h in enumerate(headers):
    col = [r[i] for r in data]
    if col and all(NUM.fullmatch(v) for v in col):
        xs = [float(v) for v in col]
        print("{}: numeric min={} max={} mean={}".format(
            h, fixed2(min(xs)), fixed2(max(xs)), fixed2(sum(xs) / len(xs))))
    else:
        longest = ""
        for v in col:
            if len(v) > len(longest):
                longest = v
        print("{}: text distinct={} longest={}".format(h, len(set(col)), longest))
