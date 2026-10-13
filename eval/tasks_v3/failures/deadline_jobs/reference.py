import os
import sys
import threading
import time

DEADLINE = 0.6
jobs = []
for line in sys.stdin.read().split("\n"):
    f = line.strip().split(" ")
    if len(f) == 3:
        jobs.append((f[0], int(f[1]), f[2]))

results = [None] * len(jobs)


def work(i, ms, outcome):
    time.sleep(ms / 1000)
    results[i] = outcome


start = time.time()
threads = []
for i, (name, ms, outcome) in enumerate(jobs):
    t = threading.Thread(target=work, args=(i, ms, outcome), daemon=True)
    t.start()
    threads.append(t)
ok = failed = late = 0
for i, (name, ms, outcome) in enumerate(jobs):
    threads[i].join(max(0, start + DEADLINE - time.time()))
    r = results[i]
    if r is None:
        late += 1
        print("{}: timed out".format(name))
    elif r.startswith("ok:"):
        ok += 1
        print("{}: ok {}".format(name, r[3:]))
    else:
        failed += 1
        print("{}: failed {}".format(name, r[5:]))
print("summary: {} ok, {} failed, {} timed out".format(ok, failed, late))
sys.stdout.flush()
os._exit(0)
