import sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    y = int(line)
    leap = (y % 4 == 0 and y % 100 != 0) or y % 400 == 0
    print(f"{y} {'leap' if leap else 'common'}")
