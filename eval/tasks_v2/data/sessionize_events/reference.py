import sys
from datetime import datetime, timezone

events = {}
for line in sys.stdin.read().splitlines():
    if not line.strip():
        continue
    ts, user, _ = line.split(" ", 2)
    t = datetime.strptime(ts, "%Y-%m-%dT%H:%M:%S%z")
    events.setdefault(user, []).append(int(t.timestamp()))


def fmt(unix):
    return datetime.fromtimestamp(unix, timezone.utc).strftime("%Y-%m-%d %H:%M:%S")


for user in sorted(events):
    times = sorted(events[user])
    sessions = [[times[0]]]
    for t in times[1:]:
        if t - sessions[-1][-1] > 1800:
            sessions.append([t])
        else:
            sessions[-1].append(t)
    print("{}: {} session(s)".format(user, len(sessions)))
    for s in sessions:
        d = s[-1] - s[0]
        print("  {} -> {} {}m{:02d}s {} events".format(fmt(s[0]), fmt(s[-1]), d // 60, d % 60, len(s)))
