import sys


def peers_ok(g, i, d):
    r, c = divmod(i, 9)
    for k in range(9):
        if g[r * 9 + k] == d or g[k * 9 + c] == d:
            return False
    br, bc = r // 3 * 3, c // 3 * 3
    for rr in range(br, br + 3):
        for cc in range(bc, bc + 3):
            if g[rr * 9 + cc] == d:
                return False
    return True


def solve(g, found):
    """Fill empty cells in row-major order with digits 1-9 ascending;
    collect up to two solutions in `found`."""
    try:
        i = g.index(0)
    except ValueError:
        found.append("".join(map(str, g)))
        return
    for d in range(1, 10):
        if peers_ok(g, i, d):
            g[i] = d
            solve(g, found)
            g[i] = 0
            if len(found) >= 2:
                return


n = 0
for line in sys.stdin.read().split("\n"):
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    n += 1
    if len(line) != 81 or any(ch not in ".123456789" for ch in line):
        print("puzzle {}: malformed".format(n))
        continue
    g = [0 if ch == "." else int(ch) for ch in line]
    bad = False
    for i in range(81):
        if g[i]:
            d, g[i] = g[i], 0
            if not peers_ok(g, i, d):
                bad = True
            g[i] = d
    if bad:
        print("puzzle {}: invalid".format(n))
        continue
    found = []
    solve(g, found)
    if not found:
        print("puzzle {}: no solution".format(n))
    else:
        print("puzzle {}: {} {}".format(n, found[0], "unique" if len(found) == 1 else "multiple"))
