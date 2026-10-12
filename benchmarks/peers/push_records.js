// Port of benchmarks/vm/push_records.fg
function build(n) {
  const recs = [];
  for (let i = 0; i < n; i++) {
    recs.push({ id: i, name: "r" + String(i) });
  }
  return recs;
}
console.log(build(100000).length);
