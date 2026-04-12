#!/usr/bin/env python3
# Benchmark: Loop - sum 1 to 1,000,000
import time

start = time.time()
total = 0
for i in range(1, 1000001):
    total += i
elapsed = (time.time() - start) * 1000
print(f"Sum = {total}")
print(f"Time: {elapsed:.0f}ms")
