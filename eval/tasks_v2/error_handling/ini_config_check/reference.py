import re
import sys

SCHEMA = [
    ("server.host", "string", None),
    ("server.port", "port", None),
    ("limits.max_conn", "count", "10"),
    ("limits.timeout", "positive", "30"),
    ("limits.debug", "bool", "false"),
]
KINDS = {name: kind for name, kind, _ in SCHEMA}


def check(kind, v):
    if kind == "string":
        return None
    if kind in ("port", "count"):
        if not re.fullmatch(r"-?\d+", v):
            return "must be an integer"
        n = int(v)
        ok = 1 <= n <= 65535 if kind == "port" else n >= 1
        return None if ok else "out of range"
    if kind == "positive":
        if not re.fullmatch(r"-?\d+(\.\d+)?", v):
            return "must be a number"
        return None if float(v) > 0 else "out of range"
    return None if v in ("true", "false") else "must be true or false"


section = None
values = {}
errors = []
for n, raw in enumerate(sys.stdin.read().split("\n"), 1):
    line = raw.strip()
    if not line or line[0] in "#;":
        continue
    m = re.fullmatch(r"\[([a-z_]+)\]", line)
    if m:
        section = m.group(1)
        continue
    m = re.fullmatch(r"([a-z_]+)\s*=\s*(.*)", line)
    if not m or not m.group(2) or section is None:
        print("line {}: syntax error".format(n))
        sys.exit(2)
    name = section + "." + m.group(1)
    if name not in KINDS:
        errors.append("line {}: unknown key {}".format(n, name))
    elif name in values:
        errors.append("line {}: duplicate key {}".format(n, name))
    else:
        values[name] = m.group(2)
        problem = check(KINDS[name], m.group(2))
        if problem:
            errors.append("line {}: {} {}".format(n, name, problem))
for name, _, default in SCHEMA:
    if default is None and name not in values:
        errors.append("missing " + name)
if errors:
    print("\n".join(errors))
    sys.exit(1)
for name, _, default in SCHEMA:
    print("{}={}".format(name, values.get(name, default)))
