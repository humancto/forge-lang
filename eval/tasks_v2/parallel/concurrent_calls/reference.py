import re
import sys
import time
from concurrent.futures import ThreadPoolExecutor

calls = [l.split() for l in sys.stdin.read().splitlines() if l.strip()]


def call(spec):
    name, ms, value = spec
    time.sleep(int(ms) / 1000)
    if re.fullmatch(r"-?\d+", value):
        return int(value) ** 2
    return None


with ThreadPoolExecutor(max_workers=len(calls)) as pool:
    results = list(pool.map(call, calls))
ok = [r for r in results if r is not None]
for (name, _, _), r in zip(calls, results):
    print("{}: {}".format(name, "failed" if r is None else r))
print("ok={} sum={}".format(len(ok), sum(ok)))
