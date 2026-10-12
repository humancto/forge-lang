import heapq
import json
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

with open("network.json") as f:
    EDGES = json.load(f)["edges"]
ADJ = {}
for e in EDGES:
    ADJ.setdefault(e["a"], []).append((e["b"], e["minutes"]))
    ADJ.setdefault(e["b"], []).append((e["a"], e["minutes"]))


def route(src, dst):
    heap = [(0, 1, [src])]
    done = set()
    while heap:
        cost, hops, path = heapq.heappop(heap)
        node = path[-1]
        if node in done:
            continue
        done.add(node)
        if node == dst:
            return {"from": src, "to": dst, "minutes": cost, "path": path}
        for nxt, m in ADJ[node]:
            if nxt not in done:
                heapq.heappush(heap, (cost + m, hops + 1, path + [nxt]))
    return {"error": "no route"}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send_json(self, obj, status=200):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        u = urlsplit(self.path)
        if u.path == "/stations":
            return self.send_json([{"name": n, "connections": len(ADJ[n])} for n in sorted(ADJ)])
        if u.path != "/route":
            return self.send_json({}, 404)
        q = {k: v[-1] for k, v in parse_qs(u.query).items()}
        if "from" not in q or "to" not in q:
            return self.send_json({"error": "from and to are required"})
        for name in (q["from"], q["to"]):
            if name not in ADJ:
                return self.send_json({"error": "unknown station: " + name})
        self.send_json(route(q["from"], q["to"]))


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18509), Handler).serve_forever()
