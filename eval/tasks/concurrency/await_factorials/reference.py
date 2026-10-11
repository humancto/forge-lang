from concurrent.futures import ThreadPoolExecutor
P = 1000000007
def fact(n):
    r = 1
    for i in range(2, n + 1):
        r = r * i % P
    return r
ns = [5, 10, 15, 20, 25]
with ThreadPoolExecutor(5) as ex:
    vals = list(ex.map(fact, ns))
for n, v in zip(ns, vals):
    print(f"{n}! mod p = {v}")
print(f"max: {max(vals)}")
