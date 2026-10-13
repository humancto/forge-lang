import json
import re
import sys

text = sys.stdin.read()
doc_text, _, queries = text.partition("\n---\n")
doc = json.loads(doc_text)
STEP = re.compile(r"\.([A-Za-z0-9_]+)|\[(-?\d+)\]")


def type_name(v):
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


def parse(path):
    m = re.match(r"[A-Za-z0-9_]+", path)
    steps, i = [], 0
    if path.startswith("."):
        return None
    if m:
        steps.append(("key", m.group(0), m.group(0)))
        i = m.end()
    while i < len(path):
        m = STEP.match(path, i)
        if not m:
            return None
        if m.group(1) is not None:
            steps.append(("key", m.group(1), m.group(0)))
        else:
            steps.append(("idx", m.group(2), m.group(0)))
        i = m.end()
    return steps or None


def evaluate(path):
    steps = parse(path)
    if steps is None:
        return "error: bad path"
    cur, loc = doc, ""
    for kind, val, raw in steps:
        where = loc or "$"
        if kind == "key":
            if not isinstance(cur, dict):
                return "error: cannot index {} at {}".format(type_name(cur), where)
            if val not in cur:
                return "error: no key '{}' at {}".format(val, where)
            cur = cur[val]
        else:
            if not isinstance(cur, list):
                return "error: cannot index {} at {}".format(type_name(cur), where)
            i = int(val)
            j = i + len(cur) if i < 0 else i
            if not 0 <= j < len(cur):
                return "error: index {} out of range at {}".format(val, where)
            cur = cur[j]
        loc += raw
    return json.dumps(cur, separators=(",", ":"), ensure_ascii=False)


for q in queries.splitlines():
    q = q.strip()
    if q:
        print("{} => {}".format(q, evaluate(q)))
