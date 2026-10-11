import re, sys
for line in sys.stdin:
    line = line.rstrip("\n")
    if not line.strip():
        continue
    print(re.sub(r"[^a-z0-9]+", "-", line.lower()).strip("-"))
