import sys
def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q
ops = {
    "add": lambda a, b: a + b, "sub": lambda a, b: a - b, "mul": lambda a, b: a * b,
    "div": tdiv, "mod": lambda a, b: a - tdiv(a, b) * b, "pow": lambda a, b: a ** b,
    "max": lambda a, b: max(a, b),
}
for line in sys.stdin:
    p = line.split()
    if len(p) != 3:
        continue
    op, a, b = p[0], int(p[1]), int(p[2])
    if op not in ops:
        print(f"unknown op: {op}")
    elif op in ("div", "mod") and b == 0:
        print(f"{op}({a}, {b}) = undefined")
    else:
        print(f"{op}({a}, {b}) = {ops[op](a, b)}")
