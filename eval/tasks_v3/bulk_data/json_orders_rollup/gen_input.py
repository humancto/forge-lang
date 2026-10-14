"""Deterministic input for json_orders_rollup: prints input.txt to stdout.

Run by the harness as `python3 gen_input.py input.txt`. Standard library
only; its own PRNG keeps the output independent of the Python version.
"""
import json
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


COUNTRIES = ["DE", "FR", "US", "JP", "BR", "IN", "NO"]
TIERS = ["free", "silver", "gold"]
NOTES = ["", "gift", "café", "über fast", "日本", "\U0001F600 thanks",
         "naïve résumé", "plain"]


def main():
    if (sys.argv[1:] or ["input.txt"])[0] != "input.txt":
        sys.exit("unknown file")
    rng = Rng(4242)
    skus = ["SKU-{:03d}".format(i) for i in range(300)]
    out = []
    for i in range(80000):
        day = 1 + rng.below(28)
        off = [0, 1, 2, -5, 9, -3][rng.below(6)]
        hh = rng.below(24)
        ts = "2026-02-{:02d}T{:02d}:{:02d}:{:02d}{}".format(
            day, hh, rng.below(60), rng.below(60),
            "Z" if off == 0 else "{}{:02d}:00".format("+" if off > 0 else "-", abs(off)))
        items = []
        for _ in range(1 + rng.below(4)):
            cents = 99 + rng.below(20000)
            items.append({"sku": skus[(rng.below(300) * rng.below(300)) // 300],
                          "qty": 1 + rng.below(5),
                          "price": cents / 100})
        order = {
            "id": "o{:06d}".format(i),
            "ts": ts,
            "user": {"id": rng.below(20000), "country": COUNTRIES[rng.below(len(COUNTRIES))],
                     "tier": TIERS[rng.below(3)]},
            "items": items,
            "coupon": [None, None, None, "SAVE10", "FREESHIP"][rng.below(5)],
            "note": NOTES[rng.below(len(NOTES))],
        }
        r = rng.below(400)
        if r == 0:
            del order["user"]["country"]
        if r == 1:
            order["items"] = []
        line = json.dumps(order, ensure_ascii=rng.below(2) == 0)
        if r == 2:
            line = line[: len(line) // 2]
        out.append(line)
    sys.stdout.write("\n".join(out) + "\n")


main()
