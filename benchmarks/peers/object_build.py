# Port of benchmarks/vm/object_build.fg
def build(n):
    ob = {}
    for i in range(0, n):
        ob["k" + str(i)] = i
    return ob


print(len(build(50000)))
