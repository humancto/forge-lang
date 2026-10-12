// Port of benchmarks/vm/records_pipeline.fg
const rows = Array.from({ length: 300000 }, (_, i) => i).map((i) => {
  return { id: i, price: (i * 7919) % 1000, qty: (i * 31) % 7 };
});
const big = rows.filter((r) => r.qty > 2);
const totals = big.map((r) => r.price * r.qty);
console.log(totals.length);
console.log(totals.reduce((acc, t) => acc + t, 0));
