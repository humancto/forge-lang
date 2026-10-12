import sys

DAY_START, DAY_END = 8 * 60, 20 * 60


def minutes(hhmm):
    h, m = hhmm.split(":")
    return int(h) * 60 + int(m)


def hhmm(m):
    return "{:02d}:{:02d}".format(m // 60, m % 60)


rooms = {}
for line in sys.stdin.read().splitlines():
    if not line.strip():
        continue
    room, span = line.split()
    s, e = span.split("-")
    rooms.setdefault(room, []).append((minutes(s), minutes(e)))

for room in sorted(rooms):
    blocks = []
    for s, e in sorted(rooms[room]):
        if blocks and s <= blocks[-1][1]:
            blocks[-1][1] = max(blocks[-1][1], e)
        else:
            blocks.append([s, e])
    busy = sum(e - s for s, e in blocks)
    best = None
    prev = DAY_START
    for s, e in blocks + [[DAY_END, DAY_END]]:
        if s - prev > 0 and (best is None or s - prev > best[1] - best[0]):
            best = (prev, s)
        prev = max(prev, e)
    free = "none" if best is None else "{}-{}".format(hhmm(best[0]), hhmm(best[1]))
    print("{} blocks={} busy={}h{:02d}m free={}".format(room, len(blocks), busy // 60, busy % 60, free))
