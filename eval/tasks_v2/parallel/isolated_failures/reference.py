import math
import re
import sys
import time
from concurrent.futures import ThreadPoolExecutor

ops = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]


class OpError(Exception):
    pass


def run(line):
    time.sleep(0.2)
    p = line.split(" ")
    if p[0] == "div":
        a, b = int(p[1]), int(p[2])
        if b == 0:
            raise OpError("division by zero")
        q = abs(a) // abs(b)
        return q if (a < 0) == (b < 0) else -q
    if p[0] == "nth":
        i, items = int(p[1]), p[2].split(",")
        if i < 0 or i >= len(items):
            raise OpError("index out of range")
        return items[i]
    if p[0] == "num":
        if not re.fullmatch(r"-?\d+", p[1]):
            raise OpError("not a number")
        return int(p[1])
    if p[0] == "isqrt":
        n = int(p[1])
        if n < 0:
            raise OpError("negative input")
        return math.isqrt(n)
    raise OpError("unknown operation")


def safe(line):
    try:
        return True, run(line)
    except OpError as e:
        return False, str(e)


with ThreadPoolExecutor(max_workers=len(ops)) as pool:
    results = list(pool.map(safe, ops))
reasons = {}
for n, (ok, v) in enumerate(results, 1):
    if ok:
        print("{}: ok {}".format(n, v))
    else:
        print("{}: error {}".format(n, v))
        reasons[v] = reasons.get(v, 0) + 1
good = sum(1 for ok, _ in results if ok)
print("ok={} errors={}".format(good, len(results) - good))
for r in sorted(reasons):
    print("{}: {}".format(r, reasons[r]))
