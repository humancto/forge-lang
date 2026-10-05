# Port of benchmarks/vm/array_push.fg
def fill(n):
    a = []
    i = 0
    while i < n:
        a.append(i)
        i = i + 1
    return a


print(len(fill(100000)))
