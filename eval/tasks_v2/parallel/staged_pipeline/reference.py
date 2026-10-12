import hashlib
import queue
import sys
import threading
import time

lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
q1, q2 = queue.Queue(maxsize=2), queue.Queue(maxsize=2)
END = object()


def parse():
    for line in lines:
        rid, _, text = line.partition(" ")
        rec = {"id": rid, "text": text, "words": len(text.split())}
        time.sleep(0.1)
        q1.put(rec)
    q1.put(END)


def digest():
    while True:
        rec = q1.get()
        if rec is END:
            q2.put(END)
            return
        rec["sha"] = hashlib.sha256(rec["text"].encode()).hexdigest()[:8]
        time.sleep(0.1)
        q2.put(rec)


threading.Thread(target=parse).start()
threading.Thread(target=digest).start()
n = 0
while True:
    rec = q2.get()
    if rec is END:
        break
    print("{} words={} sha={} {}".format(rec["id"], rec["words"], rec["sha"], rec["text"].upper()),
          flush=True)
    n += 1
    time.sleep(0.1)
print("processed {}".format(n))
