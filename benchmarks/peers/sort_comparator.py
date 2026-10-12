# Port of benchmarks/vm/sort_comparator.fg
from functools import cmp_to_key


def make(n):
    xs = []
    seed = 7
    for i in range(n):
        seed = (seed * 1103515 + 12345) % 2147483648
        xs.append(seed % 1000000)
    return xs


s = sorted(make(200000), key=cmp_to_key(lambda a, b: a - b))
print(s[0])
print(s[100000])
print(s[199999])
