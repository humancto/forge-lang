import sys
from collections import Counter
from concurrent.futures import ThreadPoolExecutor

lines = sys.stdin.read().splitlines()
chunks = [lines[i:i + 3000] for i in range(0, len(lines), 3000)]


def count(chunk):
    c = Counter()
    for line in chunk:
        c.update(line.split())
    return c


with ThreadPoolExecutor(max_workers=4) as pool:
    parts = list(pool.map(count, chunks))
total = Counter()
for k, c in enumerate(parts, 1):
    print("chunk {}: {} words, {} distinct".format(k, sum(c.values()), len(c)))
    total.update(c)
print("total: {} words, {} distinct".format(sum(total.values()), len(total)))
print("top:")
for w, n in sorted(total.items(), key=lambda kv: (-kv[1], kv[0]))[:10]:
    print("{} {}".format(w, n))
others = set().union(*(set(c) for c in parts[:3]))
print("only in chunk 4: {}".format(sum(1 for w in parts[3] if w not in others)))
