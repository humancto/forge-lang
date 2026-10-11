import csv, json, re, sys
rows = []
for r in csv.DictReader(sys.stdin):
    o = {}
    for k, v in r.items():
        if v == "":
            o[k] = None
        elif re.fullmatch(r"-?\d+", v):
            o[k] = int(v)
        elif v in ("true", "false"):
            o[k] = v == "true"
        else:
            o[k] = v
    rows.append(o)
print(json.dumps(rows, indent=2))
