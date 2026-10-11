import sys
def dsum(n):
    s = 0
    while n > 0:
        s += n % 10
        n //= 10
    return s
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    n = int(line)
    s = dsum(n)
    r = n
    while r >= 10:
        r = dsum(r)
    print(f"{n}: sum={s} root={r}")
