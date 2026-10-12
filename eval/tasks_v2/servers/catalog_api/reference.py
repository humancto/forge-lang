import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlsplit

with open("catalog.json") as f:
    PRODUCTS = json.load(f)["products"]

SORTS = {
    "price": lambda p: (p["price_cents"], p["sku"]),
    "-price": lambda p: (-p["price_cents"], p["sku"]),
    "name": lambda p: (p["name"], p["sku"]),
}


def list_products(q):
    for name in ("min_price", "max_price", "limit"):
        if name in q and not re.fullmatch(r"\d+", q[name]):
            return {"error": "invalid " + name}
    if "sort" in q and q["sort"] not in SORTS:
        return {"error": "invalid sort"}
    items = [p for p in PRODUCTS
             if ("category" not in q or p["category"] == q["category"])
             and ("min_price" not in q or p["price_cents"] >= int(q["min_price"]))
             and ("max_price" not in q or p["price_cents"] <= int(q["max_price"]))]
    items.sort(key=SORTS.get(q.get("sort"), lambda p: p["sku"]))
    return {"count": len(items), "items": items[:int(q.get("limit", "10"))]}


def categories():
    out = {}
    for p in PRODUCTS:
        c = out.setdefault(p["category"], {"category": p["category"], "count": 0,
                                           "min_price_cents": p["price_cents"],
                                           "max_price_cents": p["price_cents"]})
        c["count"] += 1
        c["min_price_cents"] = min(c["min_price_cents"], p["price_cents"])
        c["max_price_cents"] = max(c["max_price_cents"], p["price_cents"])
    return [out[k] for k in sorted(out)]


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
        q = {k: v[-1] for k, v in parse_qs(u.query).items()}
        parts = [unquote(p) for p in u.path.strip("/").split("/")]
        if parts == ["products"]:
            return self.send_json(list_products(q))
        if len(parts) == 2 and parts[0] == "products":
            for p in PRODUCTS:
                if p["sku"] == parts[1]:
                    return self.send_json(p)
            return self.send_json({"error": "not found", "sku": parts[1]})
        if parts == ["categories"]:
            return self.send_json(categories())
        self.send_json({}, 404)


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18501), Handler).serve_forever()
