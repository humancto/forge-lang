# Port of benchmarks/vm/map_filter.fg
from functools import reduce

xs = list(range(0, 1000000))
ys = list(map(lambda x: x * 2, xs))
zs = list(filter(lambda x: x % 3 == 0, ys))
print(len(zs))
print(reduce(lambda acc, x: acc + x, zs, 0))
