import sys
colors = ["red", "green", "blue", "cyan", "magenta"]
errors = 0
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    i = int(line)
    try:
        if i < 0:
            raise IndexError(i)
        print(f"{i}: {colors[i]}")
    except IndexError:
        print(f"{i}: out of range")
        errors += 1
print(f"errors: {errors}")
