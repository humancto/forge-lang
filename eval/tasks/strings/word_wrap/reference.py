import sys
data = sys.stdin.read().split("\n", 1)
w = int(data[0])
words = data[1].split()
lines, cur = [], ""
for word in words:
    if not cur:
        cur = word
    elif len(cur) + 1 + len(word) <= w:
        cur += " " + word
    else:
        lines.append(cur)
        cur = word
if cur:
    lines.append(cur)
for l in lines:
    print("|" + l.ljust(w) + "|")
