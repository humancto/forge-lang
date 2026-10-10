# Port of benchmarks/vm/toplevel_loop_fn.fg
def run():
    i = 0
    total = 0
    while i < 5000000:
        total = total + i
        i = i + 1
    return total
print(run())
