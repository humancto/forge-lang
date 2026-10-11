import sys
for line in sys.stdin:
    line = line.rstrip("\n")
    if not line:
        continue
    s = "".join(c.lower() for c in line if c.isalnum())
    print(("yes" if s == s[::-1] else "no") + " " + line)
