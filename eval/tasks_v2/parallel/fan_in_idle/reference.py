import queue
import sys
import threading
import time

lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
a_items = [int(x) for x in lines[0].split(",")]
b_items = [int(x) for x in lines[1].split(",")]
q = queue.Queue()


def producer(tag, items, hang):
    for x in items:
        time.sleep(0.02)
        q.put((tag, x))
    if hang:
        time.sleep(60)
    else:
        q.put((tag, None))


threading.Thread(target=producer, args=("a", a_items, False), daemon=True).start()
threading.Thread(target=producer, args=("b", b_items, True), daemon=True).start()
got = {"a": [], "b": []}
a_done = False
while True:
    try:
        tag, x = q.get(timeout=0.3)
    except queue.Empty:
        if a_done:
            break
        continue
    if x is None:
        a_done = True
    else:
        got[tag].append(x)
print("a: " + ",".join(map(str, got["a"])))
print("b: " + ",".join(map(str, got["b"])))
allv = got["a"] + got["b"]
print("received={} sum={}".format(len(allv), sum(allv)))
