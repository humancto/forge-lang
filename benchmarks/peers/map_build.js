// Port of benchmarks/vm/map_build.fg
const n = 100000;
const m = new Map();
for (let i = 0; i < n; i++) {
  m.set("k" + String(i), i);
}
let total = 0;
for (let j = 0; j < n; j++) {
  const k = "k" + String(j);
  if (m.has(k)) total += m.get(k);
}
console.log(m.size);
console.log(total);
