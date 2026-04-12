#!/usr/bin/env python3
# Benchmark: String concatenation - 10,000 strings
import time

start = time.time()
s = ""
for i in range(10000):
    s = s + "a"
elapsed = (time.time() - start) * 1000
print(f"String length = {len(s)}")
print(f"Time: {elapsed:.0f}ms")
