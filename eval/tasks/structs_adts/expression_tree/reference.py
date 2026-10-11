def num(n): return ("num", n)
X = ("var",)
def add(a, b): return ("add", a, b)
def mul(a, b): return ("mul", a, b)
def neg(a): return ("neg", a)
def show(e):
    k = e[0]
    if k == "num": return str(e[1])
    if k == "var": return "x"
    if k == "add": return f"({show(e[1])} + {show(e[2])})"
    if k == "mul": return f"({show(e[1])} * {show(e[2])})"
    return "-" + show(e[1])
def ev(e, x):
    k = e[0]
    if k == "num": return e[1]
    if k == "var": return x
    if k == "add": return ev(e[1], x) + ev(e[2], x)
    if k == "mul": return ev(e[1], x) * ev(e[2], x)
    return -ev(e[1], x)
exprs = [mul(add(num(2), num(3)), num(4)), add(neg(mul(X, X)), num(10)),
         mul(add(X, num(1)), add(X, num(-2)))]
for e in exprs:
    print(f"{show(e)} = {ev(e, 5)}")
