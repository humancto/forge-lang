def make_adder(n): return lambda x: x + n
def make_multiplier(n): return lambda x: x * n
def apply_all(fns, x): return [f(x) for f in fns]
a1, m2 = make_adder(1), make_multiplier(2)
fns = [make_adder(1), make_adder(10), make_multiplier(3), make_multiplier(-1), lambda x: m2(a1(x))]
for x in (0, 5, -3):
    print(f"{x} -> " + " ".join(map(str, apply_all(fns, x))))
print(f"sum of results for 7: {sum(apply_all(fns, 7))}")
