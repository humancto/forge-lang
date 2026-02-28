#!/usr/bin/env python3
# Benchmark: String concatenation - 10,000 strings

s = ""
for i in range(10000):
    s = s + "a"
print(f"String length = {len(s)}")
