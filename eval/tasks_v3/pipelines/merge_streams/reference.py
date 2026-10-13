import os
import queue
import sys
import threading
import time

specs = []
for line in sys.stdin.read().split("\n"):
    f = line.strip().split(" ")
    if len(f) == 4:
        specs.append((f[0], int(f[1]), int(f[2]), f[3] == "ends"))

q = queue.Queue()


def produce(i, name, every, count, ends):
    for k in range(1, count + 1):
        time.sleep(every / 1000)
        q.put((i, "{}-{}".format(name, k)))
    if ends:
        q.put((i, "end"))


for i, s in enumerate(specs):
    threading.Thread(target=produce, args=(i,) + s, daemon=True).start()
items = [[] for _ in specs]
ended = [False] * len(specs)
open_count = len(specs)
while open_count:
    try:
        i, v = q.get(timeout=0.5)
    except queue.Empty:
        break
    if v == "end":
        ended[i] = True
        open_count -= 1
    else:
        items[i].append(v)
for i, s in enumerate(specs):
    print("{}: {} ({} items, {})".format(s[0], ",".join(items[i]), len(items[i]),
                                         "ended" if ended[i] else "stalled"))
sys.stdout.flush()
os._exit(0)
