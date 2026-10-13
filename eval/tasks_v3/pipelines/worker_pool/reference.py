import sys
import time
from concurrent.futures import ThreadPoolExecutor

jobs = []
for line in sys.stdin.read().split("\n"):
    f = line.strip().split(" ")
    if len(f) == 3:
        jobs.append((f[0], int(f[1]), f[2]))


def work(job):
    time.sleep(job[1] / 1000)
    return "{}/{}".format(job[2].upper(), len(job[2]))


with ThreadPoolExecutor(max_workers=4) as pool:
    outs = list(pool.map(work, jobs))
for job, out in zip(jobs, outs):
    print("{} -> {}".format(job[0], out))
print("done: {} jobs".format(len(jobs)))
