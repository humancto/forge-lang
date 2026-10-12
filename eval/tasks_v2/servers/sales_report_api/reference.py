import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlsplit

with open("sales.csv") as f:
    lines = [l.strip() for l in f if l.strip()]
head = lines[0].split(",")
ROWS = []
for l in lines[1:]:
    r = dict(zip(head, l.split(",")))
    r["units"] = int(r["units"])
    r["revenue"] = r["units"] * int(r["unit_price_cents"])
    ROWS.append(r)


def report(region):
    rows = [r for r in ROWS if r["region"] == region]
    if not rows:
        return {"error": "unknown region"}
    by_sku = {}
    for r in rows:
        by_sku[r["sku"]] = by_sku.get(r["sku"], 0) + r["revenue"]
    best = sorted(by_sku.items(), key=lambda kv: (-kv[1], kv[0]))[0][0]
    return {"region": region, "orders": len(rows), "units": sum(r["units"] for r in rows),
            "revenue_cents": sum(r["revenue"] for r in rows), "best_sku": best}


def top(q):
    n = q.get("n", "3")
    if not re.fullmatch(r"\d+", n) or int(n) < 1:
        return {"error": "invalid n"}
    by = q.get("by", "revenue")
    if by not in ("units", "revenue"):
        return {"error": "invalid by"}
    prods = {}
    for r in ROWS:
        p = prods.setdefault(r["sku"], {"sku": r["sku"], "product": r["product"], "units": 0,
                                        "revenue_cents": 0})
        p["units"] += r["units"]
        p["revenue_cents"] += r["revenue"]
    key = "units" if by == "units" else "revenue_cents"
    return sorted(prods.values(), key=lambda p: (-p[key], p["sku"]))[:int(n)]


def monthly():
    months = {}
    for r in ROWS:
        m = months.setdefault(r["date"][:7], {"month": r["date"][:7], "revenue_cents": 0,
                                              "units": 0})
        m["revenue_cents"] += r["revenue"]
        m["units"] += r["units"]
    return [months[k] for k in sorted(months)]


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
        if len(parts) == 2 and parts[0] == "report":
            return self.send_json(report(parts[1]))
        if parts == ["top"]:
            return self.send_json(top(q))
        if parts == ["monthly"]:
            return self.send_json(monthly())
        self.send_json({}, 404)


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18510), Handler).serve_forever()
