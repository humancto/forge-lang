import re, sys
iv = []
for line in sys.stdin:
    m = re.fullmatch(r"(-?\d+)-(-?\d+)", line.strip())
    if m:
        iv.append((int(m.group(1)), int(m.group(2))))
iv.sort()
out = []
for a, b in iv:
    if out and a <= out[-1][1] + 1:
        out[-1][1] = max(out[-1][1], b)
    else:
        out.append([a, b])
for a, b in out:
    print(f"[{a}, {b}]")
print(f"covered: {sum(b - a + 1 for a, b in out)}")
