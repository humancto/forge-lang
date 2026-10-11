import re, sys
def parse(s):
    t = s.strip()
    if re.fullmatch(r"-?\d+", t):
        return int(t), None
    return None, f"invalid number '{t}'"
total = valid = 0
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    if not line.strip():
        continue
    v, err = parse(line)
    if err:
        print(f"line {n}: {err}")
    else:
        total += v
        valid += 1
print(f"sum: {total}")
print(f"valid: {valid}")
