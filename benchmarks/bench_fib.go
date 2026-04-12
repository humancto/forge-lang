// Benchmark: Recursive Fibonacci fib(30) — Go
// Run: go run bench_fib.go
package main

import (
	"fmt"
	"time"
)

func fib(n int) int {
	if n <= 1 {
		return n
	}
	return fib(n-1) + fib(n-2)
}

func main() {
	start := time.Now()
	result := fib(30)
	elapsed := time.Since(start).Milliseconds()
	fmt.Printf("fib(30) = %d\n", result)
	fmt.Printf("Time: %dms\n", elapsed)
}
