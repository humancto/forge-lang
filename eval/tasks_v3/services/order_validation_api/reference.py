import json
import re
import socketserver
import sqlite3
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
DB = sqlite3.connect("orders.db", check_same_thread=False)
DB.execute("CREATE TABLE IF NOT EXISTS orders (id INTEGER PRIMARY KEY AUTOINCREMENT, customer TEXT, "
           "items INTEGER, total INTEGER)")
DB.commit()


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def is_num(v):
    return (isinstance(v, (int, float))) and not isinstance(v, bool)


def validate(b):
    if not isinstance(b, dict):
        return ["body must be an object"]
    errors = []
    c = b.get("customer")
    if not isinstance(c, dict):
        errors.append("customer must be an object")
    else:
        name = c.get("name")
        if not isinstance(name, str) or not name.strip():
            errors.append("customer.name is required")
        email = c.get("email")
        if not isinstance(email, str) or not re.fullmatch(r"[^@\s]+@[^@\s]+\.[a-z]{2,}", email):
            errors.append("customer.email is invalid")
    items = b.get("items")
    if not isinstance(items, list) or not items:
        errors.append("items must be a non-empty array")
    else:
        for i, it in enumerate(items):
            if not isinstance(it, dict):
                errors.append("items[{}] must be an object".format(i))
                continue
            sku = it.get("sku")
            if not isinstance(sku, str) or not re.fullmatch(r"[A-Z]{3}-[0-9]{2}", sku):
                errors.append("items[{}].sku is invalid".format(i))
            qty = it.get("qty")
            if not is_int(qty) or not 1 <= qty <= 99:
                errors.append("items[{}].qty must be an integer from 1 to 99".format(i))
            price = it.get("price")
            if not is_num(price) or price <= 0:
                errors.append("items[{}].price must be a positive number".format(i))
    if "coupon" in b and not isinstance(b["coupon"], str):
        errors.append("coupon must be a string")
    return errors


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, obj):
        data = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        m = re.fullmatch(r"/orders/([0-9]+)", self.path)
        if m:
            with LOCK:
                r = DB.execute("SELECT id, customer, items, total FROM orders WHERE id = ?",
                               (int(m.group(1)),)).fetchone()
            if r:
                return self.send(200, {"id": r[0], "customer": r[1], "items": r[2], "total_cents": r[3]})
            return self.send(404, {"error": "no such order"})
        if re.fullmatch(r"/orders/[^/]+", self.path):
            return self.send(404, {"error": "no such order"})
        self.send(404, {"error": "not found"})

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(n) if n else b""
        if self.path != "/orders":
            return self.send(404, {"error": "not found"})
        try:
            b = json.loads(raw) if raw else {}
        except ValueError:
            return self.send(400, {"error": "invalid JSON body"})
        errors = validate(b)
        if errors:
            return self.send(422, {"errors": errors})
        total = sum(it["qty"] * round(it["price"] * 100) for it in b["items"])
        if b.get("coupon") == "HALF":
            total = (total + 1) // 2
        with LOCK:
            cur = DB.execute("INSERT INTO orders (customer, items, total) VALUES (?, ?, ?)",
                             (b["customer"]["name"].strip(), len(b["items"]), total))
            DB.commit()
        self.send(201, {"id": cur.lastrowid, "items": len(b["items"]), "total_cents": total})

    def not_allowed(self):
        self.send(405, {"error": "method not allowed"})

    do_PUT = do_DELETE = do_PATCH = not_allowed


class S(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


S(("127.0.0.1", 18526), H).serve_forever()
