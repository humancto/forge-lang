// Port of benchmarks/vm/sort_comparator.fg
function make(n) {
  const xs = [];
  let seed = 7;
  for (let i = 0; i < n; i++) {
    seed = (seed * 1103515 + 12345) % 2147483648;
    xs.push(seed % 1000000);
  }
  return xs;
}
const sorted = make(200000).sort((a, b) => a - b);
console.log(sorted[0]);
console.log(sorted[100000]);
console.log(sorted[199999]);
