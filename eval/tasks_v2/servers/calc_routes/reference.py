import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote

OPS = ("add", "sub", "mul", "div", "idiv", "mod", "pow")


def number(text):
    if not re.fullmatch(r"-?\d+(\.\d+)?", text):
        return None
    return float(text) if "." in text else int(text)


def calc(op, a_text, b_text):
    if op not in OPS:
        return {"error": "unknown op: " + op}
    for t in (a_text, b_text):
        if number(t) is None:
            return {"error": "invalid number: " + t}
    a, b = number(a_text), number(b_text)
    both_int = isinstance(a, int) and isinstance(b, int)
    if op in ("idiv", "mod") and not both_int:
        return {"error": "integers required"}
    if op in ("div", "idiv", "mod") and b == 0:
        return {"error": "division by zero"}
    if op == "add":
        r = a + b
    elif op == "sub":
        r = a - b
    elif op == "mul":
        r = a * b
    elif op == "div":
        r = a / b
    elif op == "idiv":
        q = abs(a) // abs(b)
        r = q if (a < 0) == (b < 0) else -q
    elif op == "mod":
        q = abs(a) // abs(b)
        q = q if (a < 0) == (b < 0) else -q
        r = a - b * q
    else:
        r = a ** b if both_int and b >= 0 else float(a) ** b
    return {"op": op, "a": a, "b": b, "result": r}


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
        parts = [unquote(p) for p in self.path.split("?")[0].strip("/").split("/")]
        if len(parts) == 4 and parts[0] == "calc":
            return self.send_json(calc(parts[1], parts[2], parts[3]))
        self.send_json({}, 404)


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18503), Handler).serve_forever()
