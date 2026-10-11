import threading, queue
q = queue.Queue()
def worker(k):
    for j in range(1, k * 2 + 1):
        q.put(f"w{k}:{j}")
    q.put(f"w{k}:done")
for k in (1, 2, 3):
    threading.Thread(target=worker, args=(k,)).start()
msgs, done = [], 0
while done < 3:
    m = q.get()
    if m.endswith(":done"):
        done += 1
    else:
        msgs.append(m)
for m in sorted(msgs):
    print(m)
print(f"messages: {len(msgs)}")
print(f"workers finished: {done}")
