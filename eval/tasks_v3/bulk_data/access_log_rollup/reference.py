import sys
from collections import Counter, defaultdict

requests = 0
malformed = 0
classes = Counter()
lat = defaultdict(list)
errors = Counter()
clients = Counter()
minutes = Counter()
total_bytes = 0
for line in sys.stdin.read().split("\n"):
    if line.strip() == "" or line.startswith("#"):
        continue
    f = line.split(" ")
    if len(f) != 7 or not (f[4].isdigit() and f[5].isdigit() and f[6].isdigit()):
        malformed += 1
        continue
    ts, ip, method, path, status, latency, size = f
    status, latency, size = int(status), int(latency), int(size)
    requests += 1
    path = path.split("?")[0]
    route = "/".join(":id" if s.isdigit() else s for s in path.split("/"))
    key = (method, route)
    lat[key].append(latency)
    if status >= 500:
        errors[key] += 1
    classes[status // 100] += 1
    clients[ip] += 1
    minutes[ts[:16]] += 1
    total_bytes += size

print("requests: {}".format(requests))
print("malformed: {}".format(malformed))
print("status: " + " ".join("{}xx={}".format(c, classes[c]) for c in (2, 3, 4, 5)))
print("top routes:")
keys = sorted(lat, key=lambda k: (-len(lat[k]), k[1], k[0]))[:8]
for k in keys:
    xs = sorted(lat[k])
    n = len(xs)

    def pct(q):
        return xs[(q * n + 99) // 100 - 1]

    print("{} {} n={} p50={} p95={} p99={} max={} 5xx={}".format(
        k[0], k[1], n, pct(50), pct(95), pct(99), xs[-1], errors[k]))
print("top clients:")
for ip, c in sorted(clients.items(), key=lambda kv: (-kv[1], kv[0]))[:5]:
    print("{} {}".format(ip, c))
m, c = sorted(minutes.items(), key=lambda kv: (-kv[1], kv[0]))[0]
print("peak minute: {} {}".format(m, c))
print("bytes: {}".format(total_bytes))
