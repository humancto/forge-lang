import sys
vals = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"),
        (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")]
def to_roman(n):
    out = ""
    for v, s in vals:
        while n >= v:
            out += s; n -= v
    return out
single = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100, "D": 500, "M": 1000}
def from_roman(s):
    total = 0
    for i, c in enumerate(s):
        v = single[c]
        if i + 1 < len(s) and single[s[i + 1]] > v:
            total -= v
        else:
            total += v
    return total
for line in sys.stdin:
    t = line.strip()
    if not t:
        continue
    if t.isdigit():
        print(f"{t} -> {to_roman(int(t))}")
    else:
        n = from_roman(t)
        print(f"{t} -> {n if to_roman(n) == t else 'invalid'}")
