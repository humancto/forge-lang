# Port of benchmarks/vm/toplevel_captured.fg
total = 0
def bump(n):
    global total
    total = total + n
i = 0
while i < 2000000:
    bump(i)
    i = i + 1
print(total)
