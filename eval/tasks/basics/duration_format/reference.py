import sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    s = int(line)
    h, rem = divmod(s, 3600)
    m, sec = divmod(rem, 60)
    print(f"{h}:{m:02d}:{sec:02d}")
