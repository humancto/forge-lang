import sys
rows = []
for line in sys.stdin:
    p = line.split()
    if len(p) == 2:
        rows.append((p[0], int(p[1])))
rows.sort(key=lambda r: (-r[1], r[0]))
rank = 0
for i, (n, s) in enumerate(rows):
    if i == 0 or s != rows[i - 1][1]:
        rank = i + 1
    print(f"{rank}. {n} {s}")
