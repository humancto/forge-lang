import threading, queue
q = queue.Queue()
def producer():
    for i in range(1, 21):
        q.put(i)
    q.put(None)
threading.Thread(target=producer).start()
count = total = 0
while True:
    v = q.get()
    if v is None:
        break
    count += 1
    if v % 3 == 0:
        print(f"got {v}")
        total += v * v
print(f"received: {count}")
print(f"sum of squares: {total}")
