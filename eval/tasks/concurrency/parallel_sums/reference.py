from concurrent.futures import ThreadPoolExecutor
N, K = 1000000, 4
size = N // K
def work(i):
    lo, hi = i * size + 1, (i + 1) * size
    return sum(x * x for x in range(lo, hi + 1))
with ThreadPoolExecutor(K) as ex:
    sums = list(ex.map(work, range(K)))
for i, s in enumerate(sums, 1):
    print(f"chunk {i}: {s}")
print(f"total: {sum(sums)}")
