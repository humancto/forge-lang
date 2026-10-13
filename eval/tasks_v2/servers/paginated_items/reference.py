import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

ITEMS = []
for n in range(1, 58):
    tags = ["even" if n % 2 == 0 else "odd"] + (["five"] if n % 5 == 0 else [])
    ITEMS.append({"id": n, "name": "item-{:03d}".format(n), "tags": tags})


def listing(q):
    for name in ("page", "per_page"):
        if name in q and not (re.fullmatch(r"\d+", q[name]) and int(q[name]) >= 1):
            return {"error": "invalid " + name}
    page = int(q.get("page", "1"))
    per = min(int(q.get("per_page", "10")), 25)
    tag = q.get("tag")
    matching = [i for i in ITEMS if tag is None or tag in i["tags"]]
    total = len(matching)
    pages = (total + per - 1) // per

    def link(p):
        return "/items?page={}&per_page={}".format(p, per) + ("&tag=" + tag if tag else "")

    return {"page": page, "per_page": per, "total": total, "total_pages": pages,
            "items": matching[(page - 1) * per:page * per],
            "next": link(page + 1) if page < pages else None,
            "prev": link(page - 1) if page > 1 else None}


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
        if u.path != "/items":
            return self.send_json({}, 404)
        q = {k: v[-1] for k, v in parse_qs(u.query).items()}
        self.send_json(listing(q))


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18504), Handler).serve_forever()
