// Port of benchmarks/vm/array_push.fg
function fill(n) {
  const a = [];
  let i = 0;
  while (i < n) {
    a.push(i);
    i = i + 1;
  }
  return a;
}
console.log(fill(100000).length);
