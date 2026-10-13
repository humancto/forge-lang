"""Deterministic input for network_components: prints input.txt to stdout.

Run by the harness as `python3 gen_input.py input.txt`. Standard library
only; its own PRNG keeps the output independent of the Python version.
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
    rng = Rng(5150)
    n = 120000
    out = ["nodes {}".format(n)]
    for _ in range(100000):
        a = rng.below(n)
        # Mostly local links, so there are many mid-sized components.
        b = (a + 1 + rng.below(40)) % n if rng.below(8) else rng.below(n)
        out.append("link {} {}".format(a, b))
    for _ in range(30000):
        out.append("ask {} {}".format(rng.below(n), rng.below(n)))
    sys.stdout.write("\n".join(out) + "\n")


main()
