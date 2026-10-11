def solve(n):
    sols = []
    cols = []
    def bt(r):
        if r == n:
            sols.append(list(cols)); return
        for c in range(n):
            if all(c != cc and abs(c - cc) != r - rr for rr, cc in enumerate(cols)):
                cols.append(c); bt(r + 1); cols.pop()
    bt(0)
    return sols
for n in range(1, 9):
    print(f"n={n}: {len(solve(n))}")
for c in solve(6)[0]:
    print("." * c + "Q" + "." * (6 - c - 1))
