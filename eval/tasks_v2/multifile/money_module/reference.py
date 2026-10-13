import re
import sys


def parse_money(amount, currency):
    if not re.fullmatch(r"-?[0-9]+(\.[0-9]{1,2})?", amount):
        return None, "bad amount: " + amount
    if not re.fullmatch(r"[A-Z]{3}", currency):
        return None, "bad currency: " + currency
    neg = amount.startswith("-")
    parts = amount.lstrip("-").split(".")
    cents = int(parts[0]) * 100 + (int(parts[1].ljust(2, "0")) if len(parts) == 2 else 0)
    return (-cents if neg else cents), None


def show(cents, currency):
    sign = "-" if cents < 0 else ""
    c = abs(cents)
    return "{}{}.{:02d} {}".format(sign, c // 100, c % 100, currency)


totals = {}
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    if not line.strip():
        continue
    f = line.split(" ")
    if len(f) != 6 or f[1] != "x" or f[3] != "@" or not re.fullmatch(r"-?[0-9]+", f[0]):
        print("line {}: bad format".format(n))
        continue
    cents, err = parse_money(f[4], f[5])
    if err:
        print("line {}: {}".format(n, err))
        continue
    t = totals.setdefault(f[5], [0, 0])
    t[0] += cents * int(f[0])
    t[1] += 1
for cur in sorted(totals):
    print("{}: {} over {} line(s)".format(cur, show(totals[cur][0], cur), totals[cur][1]))
