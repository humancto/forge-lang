import hashlib
import json
import os
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

STORE = "links.json"


def load():
    if not os.path.exists(STORE):
        return {}
    with open(STORE) as f:
        return json.load(f)


def save(links):
    with open(STORE, "w") as f:
        json.dump(links, f)


def stats(links):
    top = sorted(links.items(), key=lambda kv: (-kv[1]["hits"], kv[0]))[:3]
    return {"links": len(links), "total_hits": sum(l["hits"] for l in links.values()),
            "top": [{"code": c, "hits": l["hits"]} for c, l in top]}


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

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path != "/links":
            return self.send_json({}, 404)
        url = body.get("url") if isinstance(body, dict) else None
        if not isinstance(url, str) or not re.match(r"https?://", url) or " " in url:
            return self.send_json({"error": "invalid url"})
        code = hashlib.sha256(url.encode()).hexdigest()[:7]
        links = load()
        created = code not in links
        if created:
            links[code] = {"url": url, "hits": 0}
            save(links)
        self.send_json({"code": code, "url": links[code]["url"], "created": created})

    def do_GET(self):
        links = load()
        if self.path == "/stats":
            return self.send_json(stats(links))
        m = re.fullmatch(r"/links/([^/]+)", self.path)
        if not m:
            return self.send_json({}, 404)
        code = m.group(1)
        if code not in links:
            return self.send_json({"error": "unknown code"})
        links[code]["hits"] += 1
        save(links)
        self.send_json({"code": code, "url": links[code]["url"], "hits": links[code]["hits"]})

    def do_DELETE(self):
        m = re.fullmatch(r"/links/([^/]+)", self.path)
        if not m:
            return self.send_json({}, 404)
        links = load()
        code = m.group(1)
        if code not in links:
            return self.send_json({"error": "unknown code"})
        del links[code]
        save(links)
        self.send_json({"deleted": code})


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18507), Handler).serve_forever()
