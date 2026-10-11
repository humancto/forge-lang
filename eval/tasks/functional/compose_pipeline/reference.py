from functools import reduce
def compose(fs):
    return lambda x: reduce(lambda acc, f: f(acc), fs, x)
p1 = compose([lambda x: x + 3, lambda x: x * 2, lambda x: x * x])
p2 = compose([lambda x: x * x, lambda x: x - 1, lambda x: x // 2])
p3 = compose([])
for x in range(1, 6):
    print(f"x={x} p1={p1(x)} p2={p2(x)} p3={p3(x)}")
