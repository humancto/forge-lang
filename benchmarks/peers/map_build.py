# Port of benchmarks/vm/map_build.fg
n = 100000
m = {}
i = 0
while i < n:
    m["k" + str(i)] = i
    i = i + 1
total = 0
j = 0
while j < n:
    k = "k" + str(j)
    if k in m:
        total = total + m[k]
    j = j + 1
print(len(m))
print(total)
