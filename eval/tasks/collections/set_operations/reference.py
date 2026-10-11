import sys
a_line, b_line = sys.stdin.read().splitlines()[:2]
a = set(map(int, a_line.split()))
b = set(map(int, b_line.split()))
def show(s):
    return " ".join(map(str, sorted(s)))
print(("union: " + show(a | b)).rstrip())
print(("intersection: " + show(a & b)).rstrip())
print(("only first: " + show(a - b)).rstrip())
print(("only second: " + show(b - a)).rstrip())
