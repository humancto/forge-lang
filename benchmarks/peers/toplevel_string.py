# Port of benchmarks/vm/toplevel_string.fg
s = ""
i = 0
while i < 200000:
    s = s + "x"
    i = i + 1
print(len(s))
