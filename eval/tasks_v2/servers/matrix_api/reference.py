import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def is_number(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool)


def is_matrix(m):
    return (isinstance(m, list) and len(m) > 0
            and all(isinstance(r, list) and len(r) > 0 for r in m)
            and len({len(r) for r in m}) == 1
            and all(is_number(x) for r in m for x in r))


def shape(m):
    return [len(m), len(m[0])]


def multiply(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(len(b))) for j in range(len(b[0]))]
            for i in range(len(a))]


def det(m):
    if len(m) == 1:
        return m[0][0]
    total = 0
    for j in range(len(m)):
        minor = [row[:j] + row[j + 1:] for row in m[1:]]
        term = m[0][j] * det(minor)
        total = total + term if j % 2 == 0 else total - term
    return total


def handle(path, body):
    def field(name):
        v = body.get(name) if isinstance(body, dict) else None
        return v if is_matrix(v) else None

    if path == "/matrix/multiply":
        a = field("a")
        if a is None:
            return {"error": "a is not a matrix"}
        b = field("b")
        if b is None:
            return {"error": "b is not a matrix"}
        if len(a[0]) != len(b):
            return {"error": "shape mismatch: {}x{} times {}x{}".format(*(shape(a) + shape(b)))}
        r = multiply(a, b)
        return {"result": r, "shape": shape(r)}
    if path == "/matrix/transpose":
        m = field("m")
        if m is None:
            return {"error": "m is not a matrix"}
        r = [list(col) for col in zip(*m)]
        return {"result": r, "shape": shape(r)}
    if path == "/matrix/determinant":
        m = field("m")
        if m is None:
            return {"error": "m is not a matrix"}
        if len(m) != len(m[0]):
            return {"error": "matrix is not square"}
        return {"determinant": det(m)}
    return None


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
        out = handle(self.path, body)
        self.send_json(out if out is not None else {}, 200 if out is not None else 404)

    def do_GET(self):
        m = re.fullmatch(r"/matrix/identity/([^/]+)", self.path)
        if not m:
            return self.send_json({}, 404)
        n = m.group(1)
        if not re.fullmatch(r"\d+", n) or not 1 <= int(n) <= 10:
            return self.send_json({"error": "n must be between 1 and 10"})
        n = int(n)
        self.send_json({"result": [[1 if i == j else 0 for j in range(n)] for i in range(n)],
                        "shape": [n, n]})


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18506), Handler).serve_forever()
