import sys
for line in sys.stdin:
    parts = line.split()
    if len(parts) != 2:
        continue
    name, age = parts[0], int(parts[1])
    s = f"Hello, {name}! Next year you will be {age + 1}."
    if age < 18:
        s += " (minor)"
    print(s)
