import csv
import io
import sys
from decimal import Decimal

text = sys.stdin.buffer.read().decode("utf-8")
rows = list(csv.DictReader(io.StringIO(text, newline="")))


def cents(amount):
    if amount == "":
        return 0
    return int(Decimal(amount.replace(",", "")) * 100)


def fmt(c):
    return "{}.{:02d}".format(c // 100, c % 100)


totals = {}
for r in sorted(rows, key=lambda r: r["id"]):
    c = cents(r["amount"])
    totals[r["city"]] = totals.get(r["city"], 0) + c
    notes = r["notes"].replace("\r\n", "\n").replace("\n", "\\n")
    print("{}|{}|{}|{}|{}".format(r["id"], r["name"], r["city"], fmt(c), notes))
print("--")
for city, total in sorted(totals.items(), key=lambda kv: (-kv[1], kv[0])):
    print("{}: {}".format(city, fmt(total)))
