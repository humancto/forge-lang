// Port of benchmarks/vm/toplevel_captured.fg
let total = 0;
function bump(n) {
    total = total + n;
}
let i = 0;
while (i < 2000000) {
    bump(i);
    i = i + 1;
}
console.log(total);
