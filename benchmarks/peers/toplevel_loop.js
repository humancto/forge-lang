// Port of benchmarks/vm/toplevel_loop.fg
let i = 0;
let total = 0;
while (i < 5000000) {
    total = total + i;
    i = i + 1;
}
console.log(total);
