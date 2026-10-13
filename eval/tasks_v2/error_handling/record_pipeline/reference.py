import re
import sys


class Invalid(Exception):
    pass


def validate(line, seen):
    f = line.split(",")
    if len(f) != 5:
        raise Invalid("expected 5 fields, got {}".format(len(f)))
    rid, name, age, email, scores = f
    if not re.fullmatch(r"\d+", rid) or int(rid) == 0:
        raise Invalid("id: not a positive integer")
    if not re.fullmatch(r"\d+", age):
        raise Invalid("age: not an integer")
    if int(age) > 150:
        raise Invalid("age: out of range")
    parts = email.split("@")
    if len(parts) != 2 or not parts[0] or not parts[1]:
        raise Invalid("email: invalid")
    if scores == "":
        raise Invalid("scores: empty")
    vals = []
    for s in scores.split("|"):
        if not re.fullmatch(r"\d+", s) or int(s) > 100:
            raise Invalid("scores: '{}' is not a score".format(s))
        vals.append(int(s))
    if int(rid) in seen:
        raise Invalid("id: duplicate {}".format(rid))
    return int(rid), name, vals


seen = set()
valid = []
invalid = 0
lines = [l for l in sys.stdin.read().split("\n") if l.strip()]
for n, line in enumerate(lines, 1):
    try:
        rid, name, vals = validate(line, seen)
    except Invalid as e:
        invalid += 1
        print("line {}: error {}".format(n, e))
        continue
    seen.add(rid)
    total, count = sum(vals), len(vals)
    tenths = (total * 20 + count) // (2 * count)
    grade = "F"
    for g, lim in (("A", 90), ("B", 80), ("C", 70), ("D", 60)):
        if total >= lim * count:
            grade = g
            break
    valid.append((name, total, count))
    print("line {}: ok {} avg={}.{} grade={}".format(n, name, tenths // 10, tenths % 10, grade))
print("valid={} invalid={}".format(len(valid), invalid))
if valid:
    best = max(t / c for _, t, c in valid)
    names = [nm for nm, t, c in valid if t * 1.0 / c == best]
    print("best: " + ", ".join(names))
else:
    print("best: -")
