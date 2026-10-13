"""Deterministic input for customer_dedupe_join: prints input.txt to stdout.

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


FIRST = ["ada", "bo", "cyd", "dee", "eve", "finn", "gus", "hana", "ivo", "jo", "kai", "lea",
         "mo", "nia", "oli", "pia", "quin", "ria", "sam", "tia", "uma", "vic", "wes", "xia"]
LAST = ["smith", "ng", "okafor", "berg", "rossi", "kim", "silva", "novak", "haas", "diaz"]
DOMAINS = ["gmail.com", "example.org", "mail.net", "googlemail.com", "corp.io"]


def main():
    if (sys.argv[1:] or ["input.txt"])[0] != "input.txt":
        sys.exit("unknown file")
    rng = Rng(31337)
    people = []
    for i in range(40000):
        f, l = FIRST[rng.below(len(FIRST))], LAST[rng.below(len(LAST))]
        people.append((f, l, "{}.{}{}".format(f, l, i), DOMAINS[rng.below(len(DOMAINS))]))
    customers = []
    for n in range(50000):
        f, l, local, dom = people[rng.below(len(people))] if n >= 40000 else people[n]
        v = rng.below(6)
        loc = local
        if v == 1 and dom in ("gmail.com", "googlemail.com"):
            loc = local.replace(".", "")
        elif v == 2:
            loc = local + "+shop" + str(rng.below(9))
        elif v == 3:
            loc = local.upper()
        d = dom
        if dom in ("gmail.com", "googlemail.com") and v == 4:
            d = "googlemail.com" if dom == "gmail.com" else "gmail.com"
        email = "{}@{}".format(loc, d)
        if v == 5:
            email = "  " + email.capitalize() + " "
        name = "{} {}".format(f.capitalize(), l.capitalize())
        customers.append(("C{:06d}".format(100000 + n * 7 % 50000 * 3 + n % 3), name, email))
    out = ["#customers"]
    for c in customers:
        out.append(",".join(c))
    out.append("#orders")
    for o in range(220000):
        c = customers[(rng.below(50000) * rng.below(50000)) // 50000][0]
        if rng.below(500) == 0:
            c = "C999999"
        month = 1 + rng.below(12)
        day = 1 + rng.below(28)
        year = 2025 + (1 if rng.below(4) else 0)
        out.append("O{:07d},{},{},{}-{:02d}-{:02d}".format(
            o, c, 100 + rng.below(50000), year, month, day))
    sys.stdout.write("\n".join(out) + "\n")


main()
