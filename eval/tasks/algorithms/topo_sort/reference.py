import sys
deps = {}
for line in sys.stdin.read().splitlines():
    if ":" not in line:
        continue
    t, rest = line.split(":", 1)
    deps.setdefault(t.strip(), set()).update(rest.split())
for ds in list(deps.values()):
    for d in ds:
        deps.setdefault(d, set())
order, done = [], set()
while True:
    ready = sorted(t for t in deps if t not in done and deps[t] <= done)
    if not ready:
        break
    order.append(ready[0]); done.add(ready[0])
if len(order) < len(deps):
    print("cycle detected among: " + ", ".join(sorted(set(deps) - done)))
else:
    print(" -> ".join(order))
