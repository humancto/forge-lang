# Port of benchmarks/vm/string_build.fg
def build(n):
    s = ""
    i = 0
    while i < n:
        s = s + "x"
        i = i + 1
    return s


print(len(build(200000)))
