import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

EMAIL = re.compile(r"[^@\s]+@[^@\s]+\.[a-z]{2,}", re.IGNORECASE)


def check_username(v):
    if v is None:
        return "is required"
    if not isinstance(v, str) or not 3 <= len(v) <= 16:
        return "must be 3-16 characters"
    if not re.fullmatch(r"[a-z0-9_]+", v):
        return "may only contain a-z, 0-9 and _"
    if v in ("admin", "root"):
        return "is taken"
    return None


def check_email(v):
    if v is None:
        return "is required"
    if not isinstance(v, str) or not EMAIL.fullmatch(v):
        return "is invalid"
    return None


def check_age(v):
    if v is None:
        return "is required"
    if isinstance(v, bool) or not isinstance(v, int):
        return "must be an integer"
    if not 13 <= v <= 120:
        return "must be between 13 and 120"
    return None


def check_password(v):
    if v is None:
        return "is required"
    if not isinstance(v, str) or len(v) < 8:
        return "must be at least 8 characters"
    if not (re.search(r"[A-Za-z]", v) and re.search(r"[0-9]", v)):
        return "must contain a letter and a digit"
    return None


CHECKS = [("username", check_username), ("email", check_email), ("age", check_age),
          ("password", check_password)]


def signup(body):
    if not isinstance(body, dict):
        return {"ok": False, "errors": [{"field": "body", "message": "must be an object"}]}
    errors = []
    for field, check in CHECKS:
        msg = check(body.get(field))
        if msg:
            errors.append({"field": field, "message": msg})
    if errors:
        return {"ok": False, "errors": errors}
    return {"ok": True, "user": {"username": body["username"], "email": body["email"].lower(),
                                 "age": body["age"]}}


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
        if self.path != "/signup":
            return self.send_json({}, 404)
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.send_json(signup(body))

    def do_GET(self):
        self.send_json({}, 405 if self.path == "/signup" else 404)


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18502), Handler).serve_forever()
