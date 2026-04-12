// Benchmark: Recursive Fibonacci fib(30) — Rust
// Compile: rustc -O bench_fib.rs -o bench_fib_rs
use std::time::Instant;

fn fib(n: i64) -> i64 {
    if n <= 1 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    let start = Instant::now();
    let result = fib(30);
    let elapsed = start.elapsed().as_millis();
    println!("fib(30) = {result}");
    println!("Time: {elapsed}ms");
}
