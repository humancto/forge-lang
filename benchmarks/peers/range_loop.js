// Port of benchmarks/vm/range_loop.fg
let total = 0;
for (let i = 0; i < 5000000; i++) {
  total = total + i;
}
console.log(total);
