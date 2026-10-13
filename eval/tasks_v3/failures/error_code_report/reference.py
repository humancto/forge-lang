import json
import sys

# Reproduces Forge's runtime behaviour for these operations: the codes are
# Forge's stable error codes (E0004 calling a non-function, E0007 operator on
# unsupported types, E0008 division by zero, E0009 index out of bounds,
# E0010 missing key, E0011 invalid index, E0015 bad builtin argument,
# E0016 unwrap of Err, E0021 assertion failed).


class Fail(Exception):
    def __init__(self, code, message=""):
        self.code, self.message = code, message


def kind(v):
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, int):
        return "int"
    if isinstance(v, float):
        return "float"
    if isinstance(v, str):
        return "str"
    if isinstance(v, list):
        return "list"
    return "dict"


def perform(op, a, b):
    if op in ("div", "mod"):
        if kind(a) != "int" or kind(b) != "int":
            raise Fail("E0007")
        if b == 0:
            raise Fail("E0008")
        q = abs(a) // abs(b) * (1 if (a >= 0) == (b >= 0) else -1)
        return q if op == "div" else a - b * q
    if op == "index":
        if kind(a) == "list" and kind(b) == "int":
            if -len(a) <= b < len(a):
                return a[b]
            raise Fail("E0009")
        if kind(a) == "dict" and kind(b) == "str":
            if b in a:
                return a[b]
            raise Fail("E0010")
        raise Fail("E0011")
    if op == "call":
        raise Fail("E0004")
    if op == "neg":
        if kind(a) in ("int", "float"):
            return -a
        raise Fail("E0007")
    if op == "sub":
        if kind(a) == "int" and kind(b) == "int":
            return a - b
        raise Fail("E0007")
    if op == "len":
        if kind(a) in ("str", "list", "dict"):
            return len(a)
        raise Fail("E0015")
    if op == "unwrap_err":
        raise Fail("E0016", "unwrap() on Err: " + a)
    if op == "assert":
        raise Fail("E0021", "assertion failed: " + a)


counts = {}
n = 0
for line in sys.stdin.read().split("\n"):
    if not line.strip():
        continue
    n += 1
    op, rest = line.split(" ", 1)
    parts = rest.split(" | ")
    a = json.loads(parts[0])
    b = json.loads(parts[1]) if len(parts) > 1 else None
    try:
        v = perform(op, a, b)
        print("{} {}: ok {}".format(n, op, json.dumps(v, separators=(",", ":"), ensure_ascii=False)))
    except Fail as e:
        counts[e.code] = counts.get(e.code, 0) + 1
        if op in ("unwrap_err", "assert"):
            print("{} {}: {} {}".format(n, op, e.code, e.message))
        else:
            print("{} {}: {}".format(n, op, e.code))
print("errors: " + " ".join("{}={}".format(k, counts[k]) for k in sorted(counts)))
