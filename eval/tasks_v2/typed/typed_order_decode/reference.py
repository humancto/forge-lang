import json
import sys
from decimal import ROUND_HALF_UP, Decimal


class Bad(Exception):
    pass


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def field(obj, key, path, kind):
    if key not in obj:
        raise Bad("{}: missing".format(path))
    v = obj[key]
    ok = {"string": lambda x: isinstance(x, str),
          "integer": is_int,
          "number": lambda x: is_int(x) or isinstance(x, float),
          "array": lambda x: isinstance(x, list),
          "object": lambda x: isinstance(x, dict)}[kind](v)
    if not ok:
        raise Bad("{}: expected {}".format(path, kind))
    return v


def decode(el):
    if not isinstance(el, dict):
        raise Bad("$: expected object")
    oid = field(el, "id", "id", "string")
    lines = field(el, "lines", "lines", "array")
    if not lines:
        raise Bad("lines: must not be empty")
    out = []
    for i, ln in enumerate(lines):
        p = "lines[{}]".format(i)
        if not isinstance(ln, dict):
            raise Bad("{}: expected object".format(p))
        sku = field(ln, "sku", p + ".sku", "string")
        qty = field(ln, "qty", p + ".qty", "integer")
        if qty <= 0:
            raise Bad("{}.qty: must be positive".format(p))
        price = float(field(ln, "unit_price", p + ".unit_price", "number"))
        out.append((sku, qty, price))
    coupon = el.get("coupon")
    if coupon is not None and not isinstance(coupon, str):
        raise Bad("coupon: expected string")
    return oid, out, coupon


def money(x):
    return str(Decimal(x).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP))


valid = invalid = 0
revenue = 0.0
for i, el in enumerate(json.loads(sys.stdin.read())):
    try:
        oid, lines, coupon = decode(el)
    except Bad as e:
        invalid += 1
        print("#{}: invalid {}".format(i, e))
        continue
    total = 0.0
    for _, q, p in lines:
        total += q * p
    if coupon == "HALF":
        total = total / 2
    elif coupon == "TENOFF":
        total = max(0.0, total - 10)
    valid += 1
    revenue += total
    extra = " coupon={}".format(coupon) if coupon is not None else ""
    print("#{}: {} {} line(s) total={}{}".format(i, oid, len(lines), money(total), extra))
print("valid={} invalid={} revenue={}".format(valid, invalid, money(revenue)))
