// Benchmark: Recursive Fibonacci fib(30) — Node.js
// Run: node bench_fib.js

function fib(n) {
  if (n <= 1) return n;
  return fib(n - 1) + fib(n - 2);
}

const start = performance.now();
const result = fib(30);
const elapsed = Math.round(performance.now() - start);
console.log(`fib(30) = ${result}`);
console.log(`Time: ${elapsed}ms`);
