import sys
for line in sys.stdin:
    w = line.split()
    if not w:
        continue
    print(" ".join(reversed(w)) + f" ({len(w)} words)")
