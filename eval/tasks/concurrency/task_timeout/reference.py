import time
from concurrent.futures import ThreadPoolExecutor, TimeoutError
def job(secs, val):
    time.sleep(secs)
    return val
ex = ThreadPoolExecutor(2)
fast = ex.submit(job, 0.05, "fast result")
slow = ex.submit(job, 2, "slow result")
for name, fut, to in (("fast", fast, 1), ("slow", slow, 0.3)):
    try:
        print(f"{name}: {fut.result(timeout=to)}")
    except TimeoutError:
        print(f"{name}: timed out")
print(f"slow finally: {slow.result()}")
ex.shutdown()
