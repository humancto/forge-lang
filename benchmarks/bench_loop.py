#!/usr/bin/env python3
# Benchmark: Loop - sum 1 to 1,000,000

total = 0
for i in range(1, 1000001):
    total += i
print(f"Sum = {total}")
