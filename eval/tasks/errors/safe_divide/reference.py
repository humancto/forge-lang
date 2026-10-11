import sys
def divide(a, b):
    if b == 0:
        return None, "division by zero"
    q = abs(a) // abs(b)
    if (a < 0) != (b < 0):
        q = -q
    return (q, a - q * b), None
for line in sys.stdin:
    p = line.split()
    if len(p) != 2:
        continue
    a, b = int(p[0]), int(p[1])
    res, err = divide(a, b)
    if err:
        print(f"{a} / {b}: error: {err}")
    else:
        print(f"{a} / {b} = {res[0]} remainder {res[1]}")
