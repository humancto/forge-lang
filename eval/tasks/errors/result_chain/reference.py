import re, sys
class Bad(Exception): pass
def fields(line):
    p = line.split(",")
    if len(p) != 3:
        raise Bad(f"expected 3 fields, got {len(p)}")
    return p
def age(s):
    if not re.fullmatch(r"\d+", s) or int(s) > 150:
        raise Bad(f"bad age '{s}'")
    return int(s)
def email(s):
    parts = s.split("@")
    if len(parts) != 2 or not parts[0] or not parts[1]:
        raise Bad(f"bad email '{s}'")
    return s
ok = total = 0
for line in sys.stdin.read().splitlines():
    if not line:
        continue
    total += 1
    try:
        n, a, e = fields(line)
        print(f"ok: {n} ({age(a)}) {email(e)}")
        ok += 1
    except Bad as ex:
        print(f"error: {ex}")
print(f"{ok}/{total} valid")
