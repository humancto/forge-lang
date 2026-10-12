# Port of benchmarks/vm/reduce_lambda.fg
from functools import reduce

xs = list(range(0, 1000000))
print(reduce(lambda acc, x: (acc + x * x) % 1000003, xs, 0))
