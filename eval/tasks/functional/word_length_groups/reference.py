import re, sys
words = {re.sub(r"[^a-z]", "", w.lower()) for w in sys.stdin.read().split()}
words.discard("")
groups = {}
for w in words:
    groups.setdefault(len(w), []).append(w)
for n in sorted(groups):
    print(f"{n}: {' '.join(sorted(groups[n]))}")
print("longest: " + " ".join(sorted(groups[max(groups)])))
