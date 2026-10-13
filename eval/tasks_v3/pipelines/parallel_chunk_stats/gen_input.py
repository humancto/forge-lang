"""Deterministic input for parallel_chunk_stats: prints input.txt to stdout.

Run by the harness as `python3 gen_input.py input.txt`.
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
    rng = Rng(8)
    sensors = ["s{:02d}".format(i) for i in range(40)]
    out = []
    for i in range(160000):
        s = sensors[rng.below(40)]
        v = rng.below(20001) - 10000
        if rng.below(997) == 0:
            out.append("{},{}".format(s, "NaN"))
        else:
            out.append("{},{}".format(s, v))
    sys.stdout.write("\n".join(out) + "\n")


main()
