r, c = map(int, input().split())
m = [[i * c + j + 1 for j in range(c)] for i in range(r)]
top, bot, left, right = 0, r - 1, 0, c - 1
order = []
while top <= bot and left <= right:
    for j in range(left, right + 1): order.append((top, j))
    for i in range(top + 1, bot + 1): order.append((i, right))
    if top < bot:
        for j in range(right - 1, left - 1, -1): order.append((bot, j))
    if left < right:
        for i in range(bot - 1, top, -1): order.append((i, left))
    top += 1; bot -= 1; left += 1; right -= 1
print(" ".join(str(m[i][j]) for i, j in order))
out = [[0] * c for _ in range(r)]
for k, (i, j) in enumerate(order, 1):
    out[i][j] = k
w = len(str(r * c))
for row in out:
    print(" ".join(str(v).rjust(w) for v in row))
