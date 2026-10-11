import sys
for line in sys.stdin:
    line = line.rstrip("\n")
    if not line:
        continue
    k, rest = line.split(":", 1)
    xs = rest.split()
    if not xs:
        print("")
        continue
    k = int(k) % len(xs)
    print(" ".join(xs[-k:] + xs[:-k] if k else xs))
