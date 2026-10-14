"""Deterministic large input for line_diff: prints big.txt to stdout.

Run by the harness as `python3 gen_input.py big.txt`.
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
    if (sys.argv[1:] or ["big.txt"])[0] != "big.txt":
        sys.exit("unknown file")
    rng = Rng(808)
    words = ["let", "x", "=", "fn", "return", "if", "else", "{", "}", "total", "+", "1", "items"]
    a = [" ".join(words[rng.below(len(words))] for _ in range(1 + rng.below(5))) for _ in range(700)]
    b = []
    for line in a:
        r = rng.below(20)
        if r == 0:
            continue
        if r == 1:
            b.append(line + " // changed")
        elif r == 2:
            b.append(line)
            b.append("inserted {}".format(rng.below(1000)))
        else:
            b.append(line)
    sys.stdout.write("\n".join(a + ["====="] + b) + "\n")


main()
