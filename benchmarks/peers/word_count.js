// Port of benchmarks/vm/word_count.fg
const words = [];
let seed = 42;
for (let i = 0; i < 100000; i++) {
  seed = (seed * 75 + 74) % 65537;
  words.push("w" + String(seed % 5000));
}
const counts = {};
for (const w of words) {
  counts[w] = (counts[w] ?? 0) + 1;
}
console.log(Object.keys(counts).length);
