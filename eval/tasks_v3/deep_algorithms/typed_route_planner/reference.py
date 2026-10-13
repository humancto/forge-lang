import heapq
import sys

graph = {}


def dijkstra(src, metric):
    dist = {src: 0}
    prev = {}
    done = set()
    heap = [(0, src)]
    while heap:
        d, u = heapq.heappop(heap)
        if u in done:
            continue
        done.add(u)
        for v, km, mins in graph.get(u, []):
            nd = d + (km if metric == "distance" else mins)
            if v not in dist or nd < dist[v]:
                dist[v] = nd
                prev[v] = (u, km, mins)
                heapq.heappush(heap, (nd, v))
    return dist, prev


for line in sys.stdin.read().split("\n"):
    f = line.split()
    if not f:
        continue
    if f[0] == "road":
        a, b, km, mins = f[1], f[2], int(f[3]), int(f[4])
        graph.setdefault(a, []).append((b, km, mins))
        graph.setdefault(b, []).append((a, km, mins))
    elif f[0] == "route":
        a, b, metric = f[1], f[2], f[3]
        if a not in graph or b not in graph:
            print("no route from {} to {}".format(a, b))
            continue
        dist, prev = dijkstra(a, metric)
        if b not in dist:
            print("no route from {} to {}".format(a, b))
            continue
        path, km, mins, cur = [b], 0, 0, b
        while cur != a:
            p, k, m = prev[cur]
            km += k
            mins += m
            path.append(p)
            cur = p
        print("{}: {} km, {} min".format(" -> ".join(reversed(path)), km, mins))
    elif f[0] == "reachable":
        a, limit = f[1], int(f[2])
        dist, _ = dijkstra(a, "time") if a in graph else ({}, {})
        near = sorted((d, v) for v, d in dist.items() if v != a and d <= limit)
        text = "reachable from {} within {} min: {}".format(a, limit, len(near))
        if near:
            text += " (nearest: " + ", ".join("{} {}".format(v, d) for d, v in near[:5]) + ")"
        print(text)
    else:
        print("error: unknown command {}".format(f[0]))
