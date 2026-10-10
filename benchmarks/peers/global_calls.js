// Port of benchmarks/vm/global_calls.fg
function run(xs, s, n) {
    let i = 0;
    let total = 0;
    while (i < n) {
        total = total + xs.length + s.length;
        i = i + 1;
    }
    return total;
}
console.log(run([1, 2, 3], "ab", 2000000));
