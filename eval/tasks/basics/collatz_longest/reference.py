def steps(n):
    c = 0
    while n != 1:
        n = n // 2 if n % 2 == 0 else 3 * n + 1
        c += 1
    return c
best, best_n, total = -1, 0, 0
for n in range(1, 31):
    s = steps(n)
    total += s
    if s > best:
        best, best_n = s, n
print(best_n, best)
print("total", total)
