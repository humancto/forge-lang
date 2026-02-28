#!/usr/bin/env python3
# Benchmark: Factorial(20) called 10,000 times

def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)

result = 0
for i in range(10000):
    result = factorial(20)
print(f"factorial(20) = {result}")
print("Computed 10000 times")
