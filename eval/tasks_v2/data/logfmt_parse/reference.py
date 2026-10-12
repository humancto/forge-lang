import re
import sys

KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_.]*")


def parse(line):
    rec = {}
    i, n = 0, len(line)
    while True:
        while i < n and line[i] == " ":
            i += 1
        if i >= n:
            return rec
        m = KEY.match(line, i)
        if not m or m.end() >= n or line[m.end()] != "=":
            return None
        key = m.group(0)
        i = m.end() + 1
        if i < n and line[i] == '"':
            i += 1
            val = []
            while True:
                if i >= n:
                    return None
                c = line[i]
                if c == "\\" and i + 1 < n and line[i + 1] in '"\\':
                    val.append(line[i + 1])
                    i += 2
                elif c == '"':
                    i += 1
                    break
                else:
                    val.append(c)
                    i += 1
            value = "".join(val)
            if i < n and line[i] != " ":
                return None
        else:
            j = i
            while j < n and line[j] not in ' "':
                j += 1
            if j == i or (j < n and line[j] == '"'):
                return None
            value = line[i:j]
            i = j
        rec[key] = value


def dur_ms(d):
    if d.endswith("ms"):
        return int(d[:-2])
    whole, _, frac = d[:-1].partition(".")
    return int(whole) * 1000 + int((frac + "000")[:3])


levels = {}
routes = {}
invalid = 0
for line in sys.stdin.read().split("\n"):
    if not line.strip():
        continue
    rec = parse(line)
    if rec is None:
        invalid += 1
        continue
    lvl = rec["level"]
    levels[lvl] = levels.get(lvl, 0) + 1
    if lvl in ("warn", "error"):
        print("{} {} {}".format(lvl.upper(), rec["time"], rec.get("msg", "-")))
    if "route" in rec and "dur" in rec:
        routes.setdefault(rec["route"], []).append(dur_ms(rec["dur"]))
print("levels: " + " ".join("{}={}".format(k, levels[k]) for k in sorted(levels)))
for r in sorted(routes):
    ds = routes[r]
    tenths = (sum(ds) * 20 + len(ds)) // (2 * len(ds))
    print("route {}: n={} avg={}.{}ms".format(r, len(ds), tenths // 10, tenths % 10))
print("invalid: {}".format(invalid))
