import re, sys
from collections import Counter
words = re.findall(r"[a-z]+", sys.stdin.read().lower())
for w, c in sorted(Counter(words).items(), key=lambda kv: (-kv[1], kv[0]))[:5]:
    print(w, c)
