"""Deterministic input for typed_route_planner: prints input.txt to stdout.

Run by the harness as `python3 gen_input.py input.txt`. Road lengths and
times are random in 1..100000, so equal-cost alternative paths between the
queried towns do not occur (the prompt promises no ties).
"""
import sys

MASK = (1 << 64) - 1


class Rng:
    def __init__(self, seed):
        self.s = seed & MASK

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & MASK
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def below(self, n):
        return self.next() % n


def main():
    if (sys.argv[1:] or ["input.txt"])[0] != "input.txt":
        sys.exit("unknown file")
    rng = Rng(2027)
    n = 300
    names = []
    seen = set()
    while len(names) < n:
        name = "".join("bcdfghklmnprstvz"[rng.below(16)] + "aeiou"[rng.below(5)] for _ in range(3))
        if name not in seen:
            seen.add(name)
            names.append(name.capitalize())
    out = []
    edges = set()
    for i in range(1, n):
        j = rng.below(i)
        edges.add((min(i, j), max(i, j)))
    while len(edges) < 900:
        i, j = rng.below(n), rng.below(n)
        if i != j and abs(i - j) < 60:
            edges.add((min(i, j), max(i, j)))
    for i, j in sorted(edges):
        km = 1 + rng.below(100000)
        minutes = 1 + rng.below(100000)
        out.append("road {} {} {} {}".format(names[i], names[j], km, minutes))
    out.append("road Isle Atoll 5 9")
    for q in range(40):
        a, b = names[rng.below(n)], names[rng.below(n)]
        out.append("route {} {} {}".format(a, b, ["distance", "time"][q % 2]))
    out.append("route {} Isle time".format(names[0]))
    out.append("route Isle Atoll distance")
    out.append("route {} Nowhere distance".format(names[1]))
    out.append("route {} {} time".format(names[2], names[2]))
    for q in range(6):
        out.append("reachable {} {}".format(names[rng.below(n)], 20000 + rng.below(30000)))
    out.append("reachable Atoll 100")
    out.append("teleport A B")
    sys.stdout.write("\n".join(out) + "\n")


main()
