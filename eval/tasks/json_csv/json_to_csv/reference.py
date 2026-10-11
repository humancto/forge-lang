import json, sys
def q(v):
    if any(c in v for c in ',"\n'):
        return '"' + v.replace('"', '""') + '"'
    return v
print("name,city,quote")
for r in json.load(sys.stdin):
    print(",".join(q(r.get(k, "")) for k in ("name", "city", "quote")))
