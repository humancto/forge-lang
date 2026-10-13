// Port of benchmarks/vm/object_build.fg
function build(n) {
  const ob = {};
  for (let i = 0; i < n; i++) {
    ob["k" + String(i)] = i;
  }
  return ob;
}
console.log(Object.keys(build(50000)).length);
