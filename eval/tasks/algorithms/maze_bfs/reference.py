import sys
from collections import deque
grid = [list(l) for l in sys.stdin.read().splitlines() if l]
R, C = len(grid), len(grid[0])
for r in range(R):
    for c in range(C):
        if grid[r][c] == "S": s = (r, c)
        if grid[r][c] == "E": e = (r, c)
prev = {s: None}
q = deque([s])
while q:
    cur = q.popleft()
    if cur == e:
        break
    for dr, dc in ((-1, 0), (1, 0), (0, -1), (0, 1)):
        n = (cur[0] + dr, cur[1] + dc)
        if 0 <= n[0] < R and 0 <= n[1] < C and grid[n[0]][n[1]] != "#" and n not in prev:
            prev[n] = cur
            q.append(n)
if e not in prev:
    print("no path")
else:
    steps, cur = 0, prev[e]
    steps = 1
    while cur != s:
        grid[cur[0]][cur[1]] = "*"
        cur = prev[cur]
        steps += 1
    print(f"shortest path: {steps} steps")
    for row in grid:
        print("".join(row))
