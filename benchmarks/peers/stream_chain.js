// Port of benchmarks/vm/stream_chain.fg (lazy iterator chain)
const xs = Array.from({ length: 1000000 }, (_, i) => i);
function* filter(it, f) {
  for (const x of it) if (f(x)) yield x;
}
function* map(it, f) {
  for (const x of it) yield f(x);
}
function reduce(it, init, f) {
  let acc = init;
  for (const x of it) acc = f(acc, x);
  return acc;
}
const evens = filter(xs, (x) => x % 3 === 0);
console.log(reduce(map(evens, (x) => x * 2), 0, (acc, x) => acc + x));
