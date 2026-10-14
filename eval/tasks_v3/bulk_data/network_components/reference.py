import sys

lines = sys.stdin.read().split("\n")
n = int(lines[0].split()[1])
parent = list(range(n))
size = [1] * n


def find(x):
    root = x
    while parent[root] != root:
        root = parent[root]
    while parent[x] != root:
        parent[x], x = root, parent[x]
    return root


links = merges = asks = yes = 0
for line in lines[1:]:
    if not line.strip():
        continue
    op, a, b = line.split()
    a, b = int(a), int(b)
    if op == "link":
        links += 1
        ra, rb = find(a), find(b)
        if ra != rb:
            if size[ra] < size[rb] or (size[ra] == size[rb] and ra > rb):
                ra, rb = rb, ra
            parent[rb] = ra
            size[ra] += size[rb]
            merges += 1
    else:
        asks += 1
        if find(a) == find(b):
            yes += 1

comp = {}
for x in range(n):
    r = find(x)
    comp[r] = comp.get(r, 0) + 1
sizes = sorted(comp.values(), reverse=True)
print("links: {} merges: {}".format(links, merges))
print("components: {}".format(len(comp)))
print("singletons: {}".format(sum(1 for s in sizes if s == 1)))
print("largest: {}".format(" ".join(str(s) for s in sizes[:5])))
print("connected asks: {}/{}".format(yes, asks))
# The smallest node of each of the 3 largest components (ties: smallest node).
smallest = {}
for x in range(n):
    r = find(x)
    if r not in smallest:
        smallest[r] = x
top = sorted(comp, key=lambda r: (-comp[r], smallest[r]))[:3]
for r in top:
    print("component of {}: {} nodes".format(smallest[r], comp[r]))
