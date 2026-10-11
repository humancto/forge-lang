import sys
from functools import reduce
xs = list(map(int, sys.stdin.read().split()))
print("odd squares sum:", reduce(lambda a, b: a + b, map(lambda x: x * x, filter(lambda x: x % 2 != 0, xs)), 0))
print("evens doubled:", " ".join(map(lambda x: str(x * 2), filter(lambda x: x % 2 == 0, xs))))
print("product of positives:", reduce(lambda a, b: a * b, filter(lambda x: x > 0, xs), 1))
print("all below 100:", str(all(x < 100 for x in xs)).lower())
print("any negative:", str(any(x < 0 for x in xs)).lower())
