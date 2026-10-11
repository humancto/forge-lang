import json, os, socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit, parse_qs, unquote
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def send_json(self, obj, status=200):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_GET(self):
        u = urlsplit(self.path)
        if u.path.startswith("/hello/"):
            return self.send_json({"greeting": f"Hello, {unquote(u.path[7:])}!"})
        if u.path == "/add":
            q = parse_qs(u.query)
            return self.send_json({"sum": int(q["a"][0]) + int(q["b"][0])})
        self.send_json({"error": "not found"}, 404)
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        items = body["items"]
        self.send_json({"items": items[::-1], "count": len(items)})
class Server(ThreadingHTTPServer):
    # HTTPServer.server_bind resolves the host name (socket.getfqdn), a
    # reverse DNS lookup that can stall for seconds on some machines.
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]
Server(("127.0.0.1", 18471), H).serve_forever()
