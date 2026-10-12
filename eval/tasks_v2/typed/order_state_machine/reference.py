import re
import sys

orders = {}


def name(state):
    return state[0]


def label(state):
    kind = state[0]
    if kind in ("created", "paid"):
        return "{}({})".format(kind, state[1])
    if kind == "shipped":
        return "shipped({})".format(state[2])
    if kind == "delivered":
        return "delivered"
    return "cancelled({}, refund {})".format(state[1], state[2])


def step(state, verb, args):
    kind = state[0]
    if verb == "pay" and kind == "created":
        if args[0] != str(state[1]):
            return None, "amount mismatch: expected {}".format(state[1])
        return ("paid", state[1]), None
    if verb == "ship" and kind == "paid":
        return ("shipped", state[1], args[0]), None
    if verb == "deliver" and kind == "shipped":
        return ("delivered", state[1]), None
    if verb == "cancel" and kind in ("created", "paid"):
        return ("cancelled", " ".join(args), state[1] if kind == "paid" else 0), None
    return None, "cannot {} a {} order".format(verb, kind)


for line in sys.stdin.read().splitlines():
    if not line.strip():
        continue
    p = line.split()
    verb, oid, args = p[0], p[1], p[2:]
    if verb == "create":
        if oid in orders:
            print("{}: error: order exists".format(oid))
        elif not re.fullmatch(r"\d+", args[0]) or int(args[0]) == 0:
            print("{}: error: bad amount".format(oid))
        else:
            orders[oid] = ("created", int(args[0]))
            print("{}: new -> created".format(oid))
        continue
    if verb not in ("pay", "ship", "deliver", "cancel"):
        print("{}: error: unknown command".format(oid))
        continue
    if oid not in orders:
        print("{}: error: unknown order".format(oid))
        continue
    new, err = step(orders[oid], verb, args)
    if err:
        print("{}: error: {}".format(oid, err))
    else:
        print("{}: {} -> {}".format(oid, name(orders[oid]), name(new)))
        orders[oid] = new
print("summary:")
revenue = 0
for oid in sorted(orders):
    st = orders[oid]
    print("  {} {}".format(oid, label(st)))
    if st[0] in ("paid", "shipped", "delivered"):
        revenue += st[1]
print("revenue: {}".format(revenue))
