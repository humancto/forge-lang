import sys
from concurrent.futures import ThreadPoolExecutor
words = sys.stdin.read().split()
with ThreadPoolExecutor(8) as ex:
    res = list(ex.map(lambda w: f"{w}={len(set(w.lower()))}", words))
print(", ".join(res))
print(f"tasks: {len(words)}")
