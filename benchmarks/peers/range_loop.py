# Port of benchmarks/vm/range_loop.fg
total = 0
for i in range(0, 5000000):
    total = total + i
print(total)
