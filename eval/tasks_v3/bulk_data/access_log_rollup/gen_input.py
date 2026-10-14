"""Deterministic input for access_log_rollup: prints input.txt to stdout.

Run by the harness as `python3 gen_input.py input.txt` (see eval/README.md,
"Generated inputs"). Standard library only; uses its own PRNG so the output
never depends on the Python version.
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


ROUTES = [
    ("GET", "/api/users/{}", 30), ("GET", "/api/users", 12), ("POST", "/api/users", 4),
    ("GET", "/api/orders/{}", 20), ("PUT", "/api/orders/{}", 5), ("DELETE", "/api/orders/{}", 2),
    ("GET", "/api/orders/{}/items/{}", 9), ("GET", "/health", 10), ("POST", "/api/login", 6),
    ("GET", "/static/app.js", 7), ("GET", "/api/search", 8), ("PATCH", "/api/users/{}", 3),
]


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "input.txt"
    if name != "input.txt":
        sys.exit("unknown file " + name)
    rng = Rng(20261013)
    total_weight = sum(w for _, _, w in ROUTES)
    ips = ["10.0.{}.{}".format(rng.below(8), rng.below(250) + 1) for _ in range(400)]
    out = []
    t = 1772323200  # 2026-03-01T00:00:00Z
    n = 150000
    for i in range(n):
        t += rng.below(3)
        r = rng.below(total_weight)
        for method, pattern, w in ROUTES:
            if r < w:
                break
            r -= w
        path = pattern.format(*[rng.below(5000) + 1 for _ in range(pattern.count("{}"))])
        if pattern == "/api/search":
            path += "?q=w" + str(rng.below(100))
        roll = rng.below(1000)
        status = 200
        if roll < 12:
            status = 500 + rng.below(4)
        elif roll < 60:
            status = [400, 401, 403, 404, 404, 429][rng.below(6)]
        elif roll < 90:
            status = [301, 302, 304][rng.below(3)]
        elif method == "POST" and roll < 400:
            status = 201
        latency = 2 + rng.below(40) + (rng.below(900) if rng.below(50) == 0 else 0)
        if status >= 500:
            latency += 200 + rng.below(800)
        ip = ips[(rng.below(400) * rng.below(400)) // 400]
        size = rng.below(20000)
        secs = t % 86400
        day = 1 + (t - 1772323200) // 86400
        ts = "2026-03-{:02d}T{:02d}:{:02d}:{:02d}Z".format(day, secs // 3600, secs // 60 % 60, secs % 60)
        line = "{} {} {} {} {} {} {}".format(ts, ip, method, path, status, latency, size)
        bad = rng.below(2000)
        if bad == 0:
            line = "{} {} {} {}".format(ts, ip, method, path)
        elif bad == 1:
            line = "{} {} {} {} {} slow {}".format(ts, ip, method, path, status, size)
        elif bad == 2:
            line = ""
        elif bad == 3:
            line = "# rotated " + ts
        out.append(line)
    sys.stdout.write("\n".join(out) + "\n")


main()
