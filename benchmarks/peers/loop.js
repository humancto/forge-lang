// Port of benchmarks/vm/loop.fg
function count(n) {
  let i = 0;
  let total = 0;
  while (i < n) {
    total = total + i;
    i = i + 1;
  }
  return total;
}
console.log(count(20000000));
