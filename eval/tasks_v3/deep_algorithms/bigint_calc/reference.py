import re
import sys

NUM = re.compile(r"-?[0-9]+")
for line in sys.stdin.read().split("\n"):
    f = line.split()
    if not f:
        continue
    try:
        if len(f) == 2 and f[0] == "fact" and re.fullmatch(r"[0-9]+", f[1]):
            r = 1
            for i in range(2, int(f[1]) + 1):
                r *= i
        elif len(f) == 3 and f[1] in "+-*" and len(f[1]) == 1 and NUM.fullmatch(f[0]) and NUM.fullmatch(f[2]):
            a, b = int(f[0]), int(f[2])
            r = a + b if f[1] == "+" else a - b if f[1] == "-" else a * b
        elif len(f) == 3 and f[1] == "^" and NUM.fullmatch(f[0]) and re.fullmatch(r"[0-9]+", f[2]):
            r = int(f[0]) ** int(f[2])
        else:
            raise ValueError
    except ValueError:
        print("error: bad expression")
        continue
    s = str(r)
    digits = len(s.lstrip("-"))
    print("{} ({} digits, digit sum {})".format(s, digits, sum(int(c) for c in s.lstrip("-"))))
