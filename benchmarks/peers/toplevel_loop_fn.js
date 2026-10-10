// Port of benchmarks/vm/toplevel_loop_fn.fg
function run() {
    let i = 0;
    let total = 0;
    while (i < 5000000) {
        total = total + i;
        i = i + 1;
    }
    return total;
}
console.log(run());
