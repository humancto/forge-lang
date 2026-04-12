#!/usr/bin/env python3
# Benchmark: Array operations - create 100,000 items, map, filter, reduce
import time
from functools import reduce

start = time.time()

# Build array with a loop (fair comparison — no C-level range())
arr = []
for i in range(100000):
    arr.append(i)
print(f"Array created, length = {len(arr)}")

# Map: double each value
doubled = list(map(lambda x: x * 2, arr))
print(f"Map done, length = {len(doubled)}")

# Filter: keep even values
evens = list(filter(lambda x: x % 2 == 0, doubled))
print(f"Filter done, length = {len(evens)}")

# Reduce: sum
total = reduce(lambda acc, x: acc + x, evens, 0)
elapsed = (time.time() - start) * 1000
print(f"Reduce sum = {total}")
print(f"Time: {elapsed:.0f}ms")
