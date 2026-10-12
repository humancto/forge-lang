import sys
import threading
import time

jobs = [l.split() for l in sys.stdin.read().splitlines() if l.strip()]
results = {}


def run(name, ms):
    time.sleep(int(ms) / 1000)
    results[name] = "{}-{}".format(name.upper(), ms)


threads = []
for name, ms in jobs:
    t = threading.Thread(target=run, args=(name, ms), daemon=True)
    t.start()
    threads.append(t)
deadline = time.monotonic() + 1.0
for t in threads:
    t.join(max(0.0, deadline - time.monotonic()))
done = 0
for name, _ in jobs:
    if name in results:
        done += 1
        print("{}: {}".format(name, results[name]))
    else:
        print("{}: timed out".format(name))
print("done={} timed_out={}".format(done, len(jobs) - done))
