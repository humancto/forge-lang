import re, sys
found = re.findall(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", sys.stdin.read())
uniq = sorted({e.lower() for e in found})
for e in uniq:
    print(e)
print(f"total: {len(uniq)}")
