# Port of benchmarks/vm/loop.fg
def count(n):
    i = 0
    total = 0
    while i < n:
        total = total + i
        i = i + 1
    return total


print(count(20000000))
