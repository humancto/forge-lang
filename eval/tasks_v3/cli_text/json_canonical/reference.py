import json
import sys


def kind(v):
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "boolean"
    if isinstance(v, (int, float)):
        return "number"
    if isinstance(v, str):
        return "string"
    if isinstance(v, list):
        return "array"
    return "object"


def depth(v):
    if isinstance(v, list):
        return 1 + max([depth(x) for x in v] or [0])
    if isinstance(v, dict):
        return 1 + max([depth(x) for x in v.values()] or [0])
    return 0


def count_numbers(v):
    if isinstance(v, bool) or v is None or isinstance(v, str):
        return 0
    if isinstance(v, (int, float)):
        return 1
    items = v if isinstance(v, list) else v.values()
    return sum(count_numbers(x) for x in items)


for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    if not line.strip():
        continue
    try:
        v = json.loads(line)
    except ValueError:
        print("{}: invalid".format(n))
        continue
    print("{}: {} depth={} numbers={} {}".format(n, kind(v), depth(v), count_numbers(v),
                                              json.dumps(v, ensure_ascii=False, separators=(",", ":"))))
