// Port of benchmarks/vm/repeat_loop.fg
let total = 0;
for (let k = 0; k < 5000000; k++) {
  total = total + 3;
}
console.log(total);
