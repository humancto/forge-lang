import sys

current = {}
counts = {}
for idx, line in enumerate(sys.stdin.read().splitlines()):
    if not line.strip():
        continue
    ts, user, plan = line.split()
    key = (int(ts), idx)
    counts[user] = counts.get(user, 0) + 1
    if user not in current or key > current[user][0]:
        current[user] = (key, plan)

print("users: {}".format(len(current)))
per_plan = {}
for _, plan in current.values():
    per_plan[plan] = per_plan.get(plan, 0) + 1
for plan in sorted(per_plan):
    print("{}: {}".format(plan, per_plan[plan]))
print("most updates:")
for user in sorted(counts, key=lambda u: (-counts[u], u))[:5]:
    print("{} {} {}".format(user, counts[user], current[user][1]))
