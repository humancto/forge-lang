import re
import sys


def parse(line):
    p = line.split()
    kind = p[0]
    if kind not in ("recv", "ship", "adjust", "audit"):
        return None, "unknown event " + kind
    if kind in ("recv", "ship"):
        if len(p) != 3:
            return None, "wrong number of fields"
        if not re.fullmatch(r"\d+", p[2]) or int(p[2]) == 0:
            return None, "bad quantity " + p[2]
        return (kind, p[1], int(p[2])), None
    if kind == "adjust":
        if len(p) < 4:
            return None, "wrong number of fields"
        if not re.fullmatch(r"[+-]?\d+", p[2]) or int(p[2]) == 0:
            return None, "bad quantity " + p[2]
        return (kind, p[1], int(p[2]), " ".join(p[3:])), None
    if len(p) != 2:
        return None, "wrong number of fields"
    return (kind, p[1]), None


stock = {}
adjustments = []
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    if not line.strip():
        continue
    ev, err = parse(line)
    if err:
        print("line {}: parse error: {}".format(n, err))
        continue
    kind, sku = ev[0], ev[1]
    have = stock.get(sku, 0)
    if kind == "recv":
        stock[sku] = have + ev[2]
    elif kind == "ship":
        if ev[2] > have:
            print("line {}: rejected: only {} {} in stock".format(n, have, sku))
        else:
            stock[sku] = have - ev[2]
    elif kind == "adjust":
        if have + ev[2] < 0:
            print("line {}: rejected: adjustment would make {} negative".format(n, sku))
        else:
            stock[sku] = have + ev[2]
            adjustments.append("{} {:+d} ({})".format(sku, ev[2], ev[3]))
    else:
        print("audit {}: {}".format(sku, have))
print("stock:")
for sku in sorted(stock):
    if stock[sku]:
        print("  {} {}".format(sku, stock[sku]))
print("adjustments:")
for a in adjustments or ["none"]:
    print("  " + a)
