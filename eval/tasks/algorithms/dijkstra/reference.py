import heapq, sys
graph, start = {}, None
for line in sys.stdin.read().splitlines():
    p = line.split()
    if len(p) == 3:
        a, b, w = p[0], p[1], int(p[2])
        graph.setdefault(a, []).append((b, w))
        graph.setdefault(b, []).append((a, w))
    elif len(p) == 2 and p[0] == "from":
        start = p[1]
dist, prev = {start: 0}, {start: None}
pq = [(0, start)]
done = set()
while pq:
    d, u = heapq.heappop(pq)
    if u in done:
        continue
    done.add(u)
    for v, w in graph.get(u, []):
        if v not in dist or d + w < dist[v]:
            dist[v] = d + w
            prev[v] = u
            heapq.heappush(pq, (d + w, v))
for c in sorted(graph):
    if c not in dist:
        print(f"{c}: unreachable")
        continue
    path, cur = [], c
    while cur is not None:
        path.append(cur)
        cur = prev[cur]
    print(f"{c}: {dist[c]} via {'>'.join(reversed(path))}")
