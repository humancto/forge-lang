import sys

text = sys.stdin.read()
lines = text.split("\n")
if lines and lines[-1] == "":
    lines.pop()
sep = lines.index("=====")
a, b = lines[:sep], lines[sep + 1:]
n, m = len(a), len(b)
# dp[i][j]: length of the longest common subsequence of a[i:] and b[j:].
dp = [[0] * (m + 1) for _ in range(n + 1)]
for i in range(n - 1, -1, -1):
    row, nxt = dp[i], dp[i + 1]
    ai = a[i]
    for j in range(m - 1, -1, -1):
        if ai == b[j]:
            row[j] = nxt[j + 1] + 1
        else:
            row[j] = nxt[j] if nxt[j] >= row[j + 1] else row[j + 1]
i = j = 0
deleted = inserted = 0
while i < n or j < m:
    if i < n and j < m and a[i] == b[j]:
        i += 1
        j += 1
    elif j >= m or (i < n and dp[i + 1][j] >= dp[i][j + 1]):
        print("-{}: {}".format(i + 1, a[i]))
        deleted += 1
        i += 1
    else:
        print("+{}: {}".format(j + 1, b[j]))
        inserted += 1
        j += 1
if deleted or inserted:
    sys.stderr.write("{} deleted, {} inserted, {} unchanged\n".format(deleted, inserted, dp[0][0]))
    sys.exit(1)
sys.stderr.write("no differences ({} lines)\n".format(n))
