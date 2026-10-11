import json, sys
a, b = sys.stdin.read().split("\n---\n")
base, over = json.loads(a), json.loads(b)
def merge(x, y):
    out = dict(x)
    for k, v in y.items():
        if v is None:
            out.pop(k, None)
        elif isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = merge(out[k], v)
        else:
            out[k] = v
    return out
def leaves(o, prefix):
    for k in sorted(o):
        v = o[k]
        p = prefix + k
        if isinstance(v, dict) and v:
            leaves(v, p + ".")
        else:
            print(f"{p} = {json.dumps(v, separators=(',', ':'))}")
leaves(merge(base, over), "")
