import sys
from datetime import date, timedelta

lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
holidays = {date.fromisoformat(d.strip()) for d in lines[0].split(":", 1)[1].split(",")}


def business(d):
    return d.weekday() < 5 and d not in holidays


def between(a, b):
    if b < a:
        return -between(b, a)
    n = 0
    while a < b:
        n += business(a)
        a += timedelta(days=1)
    return n


def add(d, n):
    count = 0
    while count < n:
        d += timedelta(days=1)
        if business(d):
            count += 1
    return d


for q in lines[1:]:
    parts = q.split()
    if parts[0] == "between":
        ans = between(date.fromisoformat(parts[1]), date.fromisoformat(parts[2]))
    else:
        ans = add(date.fromisoformat(parts[1]), int(parts[2])).isoformat()
    print("{}: {}".format(q, ans))
