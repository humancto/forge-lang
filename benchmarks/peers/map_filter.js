// Port of benchmarks/vm/map_filter.fg
const xs = Array.from({ length: 1000000 }, (_, i) => i);
const ys = xs.map((x) => x * 2);
const zs = ys.filter((x) => x % 3 === 0);
console.log(zs.length);
console.log(zs.reduce((acc, x) => acc + x, 0));
