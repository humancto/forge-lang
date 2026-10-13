import sys

all_words = []
n = 0
for line in sys.stdin.read().split("\n"):
    if not line.strip():
        continue
    n += 1
    words = line.split()
    all_words.extend(words)
    cut = line[:12]
    print("{}: chars={} words={} first3={} padded=[{}]".format(
        n, len(line), len(words), line[:3], cut + "." * (12 - len(cut))))
    print("   upper={}".format(line.upper()))
    print("   reversed={}".format(line[::-1]))
distinct = sorted(set(w.lower() for w in all_words))
longest = ""
for w in all_words:
    if len(w) > len(longest):
        longest = w
print("words: {} distinct: {}".format(len(all_words), len(distinct)))
print("longest: {} ({} chars)".format(longest, len(longest)))
print("sorted: {}".format(" ".join(distinct)))
