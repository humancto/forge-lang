import sys
from collections import defaultdict


def merge(iv):
    iv.sort()
    blocks = []
    for s, e in iv:
        if blocks and s <= blocks[-1][1]:
            if e > blocks[-1][1]:
                blocks[-1][1] = e
        else:
            blocks.append([s, e])
    return blocks


count = skipped = 0
by_room = defaultdict(list)
events = []
for line in sys.stdin.read().split("\n"):
    if not line.strip():
        continue
    f = line.split()
    if len(f) != 3 or not f[1].isdigit() or not f[2].isdigit() or int(f[2]) <= int(f[1]):
        skipped += 1
        continue
    s, e = int(f[1]), int(f[2])
    count += 1
    by_room[f[0]].append((s, e))
    events.append((s, 1))
    events.append((e, -1))

events.sort()  # at equal times the end (-1) comes first: intervals are half-open
cur = best = 0
best_t = 0
for t, d in events:
    cur += d
    if cur > best:
        best, best_t = cur, t

stats = []
all_iv = []
for room, iv in by_room.items():
    blocks = merge(list(iv))
    busy = sum(e - s for s, e in blocks)
    longest = max(e - s for s, e in blocks)
    stats.append((busy, room, len(blocks), longest))
    all_iv.extend(iv)

print("intervals: {}".format(count))
print("skipped: {}".format(skipped))
print("rooms: {}".format(len(by_room)))
print("max overlap: {} at {}".format(best, best_t))
print("top rooms:")
for busy, room, nb, longest in sorted(stats, key=lambda x: (-x[0], x[1]))[:5]:
    print("{} busy={} blocks={} longest={}".format(room, busy, nb, longest))
union = merge(all_iv)
print("union: {} in {} blocks".format(sum(e - s for s, e in union), len(union)))
