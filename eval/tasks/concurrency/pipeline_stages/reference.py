import sys, threading, queue
lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
q1, q2 = queue.Queue(), queue.Queue()
def s1():
    for l in lines:
        q1.put(l)
    q1.put(None)
def s2():
    while True:
        l = q1.get()
        if l is None:
            q2.put(None)
            return
        q2.put(f"{l.upper()} ({len(l)})")
threading.Thread(target=s1).start()
threading.Thread(target=s2).start()
i = 0
while True:
    r = q2.get()
    if r is None:
        break
    i += 1
    print(f"{i}: {r}")
print(f"done: {i}")
