# Port of benchmarks/vm/word_count.fg
words = []
seed = 42
for i in range(0, 100000):
    seed = (seed * 75 + 74) % 65537
    words.append("w" + str(seed % 5000))
counts = {}
for w in words:
    counts[w] = counts.get(w, 0) + 1
print(len(counts))
