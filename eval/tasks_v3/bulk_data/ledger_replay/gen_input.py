"""Deterministic input for ledger_replay: prints input.txt to stdout.

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
    rng = Rng(77)
    accounts = ["AC{:05d}".format(rng.below(90000) + 10000) for _ in range(6000)]
    out = []
    for _ in range(200000):
        a = accounts[rng.below(len(accounts))]
        r = rng.below(100)
        if r < 40:
            out.append("deposit {} {}".format(a, rng.below(50000) + 1))
        elif r < 65:
            out.append("withdraw {} {}".format(a, rng.below(40000) + 1))
        elif r < 95:
            b = accounts[rng.below(len(accounts))]
            out.append("transfer {} {} {}".format(a, b, rng.below(30000) + 1))
        elif r < 97:
            out.append("freeze {}".format(a))
        elif r < 99:
            out.append("unfreeze {}".format(a))
        else:
            out.append(["withdraw {} -5".format(a), "deposit {}".format(a),
                        "refund {} 10".format(a), "transfer {} {} 0".format(a, a)][rng.below(4)])
    sys.stdout.write("\n".join(out) + "\n")


main()
