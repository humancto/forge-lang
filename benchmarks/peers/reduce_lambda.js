// Port of benchmarks/vm/reduce_lambda.fg
const xs = Array.from({ length: 1000000 }, (_, i) => i);
console.log(xs.reduce((acc, x) => (acc + x * x) % 1000003, 0));
