import re
import sys
from collections import Counter

words = [w.lower() for w in re.findall(r"[A-Za-z]+", sys.stdin.read())]
counts = Counter(words)
print("words: {}".format(len(words)))
print("distinct: {}".format(len(counts)))
print("once: {}".format(sum(1 for c in counts.values() if c == 1)))
for w, c in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:10]:
    print("{} {}".format(w, c))
