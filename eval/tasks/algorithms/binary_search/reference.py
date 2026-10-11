import sys
lines = sys.stdin.read().split("\n")
xs = list(map(int, lines[0].split()))
comps = 0
for line in lines[1:]:
    if not line.strip():
        continue
    q = int(line)
    lo, hi, found = 0, len(xs), -1
    while lo < hi:
        comps += 1
        mid = (lo + hi) // 2
        if xs[mid] == q:
            found = mid
            break
        if xs[mid] < q:
            lo = mid + 1
        else:
            hi = mid
    print(f"{q}: found at {found}" if found >= 0 else f"{q}: not found, insert at {lo}")
print(f"comparisons: {comps}")
