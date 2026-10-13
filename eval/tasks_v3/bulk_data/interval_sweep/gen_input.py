"""Deterministic input for interval_sweep: prints input.txt to stdout.

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
    rng = Rng(99)
    rooms = ["room-{}".format(chr(97 + i % 26) + str(i)) for i in range(150)]
    out = []
    for _ in range(150000):
        room = rooms[rng.below(len(rooms))]
        start = rng.below(10000000)
        length = 1 + rng.below(3000) if rng.below(10) else 1 + rng.below(60000)
        end = min(start + length, 10000000)
        r = rng.below(1000)
        if r == 0:
            out.append("{} {} {}".format(room, end, start))
        elif r == 1:
            out.append("{} {} x{}".format(room, start, end))
        elif r == 2:
            out.append("{} {} {}".format(room, start, start))
        else:
            out.append("{} {} {}".format(room, start, end))
    sys.stdout.write("\n".join(out) + "\n")


main()
