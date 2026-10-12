# Port of benchmarks/vm/stream_chain.fg (lazy iterator chain)
from functools import reduce

xs = list(range(0, 1000000))
total = reduce(
    lambda acc, x: acc + x,
    map(lambda x: x * 2, filter(lambda x: x % 3 == 0, iter(xs))),
    0,
)
print(total)
