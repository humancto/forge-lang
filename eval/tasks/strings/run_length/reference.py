import re, sys
for line in sys.stdin:
    line = line.rstrip("\n")
    if len(line) < 1:
        continue
    op, s = line[0], line[2:]
    if op == "E":
        out = []
        i = 0
        while i < len(s):
            j = i
            while j < len(s) and s[j] == s[i]:
                j += 1
            out.append(f"{s[i]}{j - i}")
            i = j
        print("".join(out))
    else:
        print("".join(c * int(n) for c, n in re.findall(r"(\D)(\d+)", s)))
