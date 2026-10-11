import sys
digits = "0123456789abcdefghijklmnopqrstuvwxyz"
def conv(n, b):
    if n == 0: return "0"
    out = ""
    while n:
        out = digits[n % b] + out
        n //= b
    return out
last = None
for line in sys.stdin:
    if not line.strip():
        continue
    n = int(line)
    last = n
    print(f"{n}: bin={conv(n, 2)} oct={conv(n, 8)} hex={conv(n, 16)} b36={conv(n, 36)}")
print("roundtrip: " + ("ok" if int(conv(last, 36), 36) == last else "fail"))
