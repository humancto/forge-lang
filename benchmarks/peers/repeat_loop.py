# Port of benchmarks/vm/repeat_loop.fg
total = 0
for _ in range(5000000):
    total = total + 3
print(total)
