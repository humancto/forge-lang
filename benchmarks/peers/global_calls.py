# Port of benchmarks/vm/global_calls.fg
def run(xs, s, n):
    i = 0
    total = 0
    while i < n:
        total = total + len(xs) + len(s)
        i = i + 1
    return total
print(run([1, 2, 3], "ab", 2000000))
