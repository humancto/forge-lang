import json
import re
import socketserver
import sqlite3
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
DB = sqlite3.connect("bank.db", check_same_thread=False)
DB.execute("CREATE TABLE IF NOT EXISTS accounts (id INTEGER PRIMARY KEY AUTOINCREMENT, "
           "owner TEXT NOT NULL, currency TEXT NOT NULL, balance INTEGER NOT NULL)")
DB.execute("CREATE TABLE IF NOT EXISTS transfers (id INTEGER PRIMARY KEY AUTOINCREMENT, "
           "src INTEGER NOT NULL, dst INTEGER NOT NULL, amount INTEGER NOT NULL)")
DB.commit()


def account(i):
    row = DB.execute("SELECT id, owner, currency, balance FROM accounts WHERE id = ?", (i,)).fetchone()
    if row is None:
        return None
    return {"id": row[0], "owner": row[1], "currency": row[2], "balance": row[3]}


def pos_int(v):
    return isinstance(v, int) and not isinstance(v, bool) and v > 0


def path_id(s):
    return int(s) if re.fullmatch(r"[0-9]+", s) else -1


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, obj, headers=None):
        data = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(n) if n else b""
        try:
            return json.loads(raw) if raw else {}
        except ValueError:
            return None

    def do_GET(self):
        with LOCK:
            m = re.fullmatch(r"/accounts/([^/]+)", self.path)
            if m:
                a = account(path_id(m.group(1)))
                return self.send(200, a) if a else self.send(404, {"error": "account not found"})
            m = re.fullmatch(r"/accounts/([^/]+)/transfers", self.path)
            if m:
                i = path_id(m.group(1))
                if account(i) is None:
                    return self.send(404, {"error": "account not found"})
                rows = DB.execute("SELECT id, src, dst, amount FROM transfers WHERE src = ? OR dst = ? "
                                  "ORDER BY id", (i, i)).fetchall()
                return self.send(200, {"transfers": [
                    {"id": r[0], "from": r[1], "to": r[2], "amount": r[3]} for r in rows]})
            if self.path == "/stats":
                n, total = DB.execute("SELECT COUNT(*), COALESCE(SUM(balance), 0) FROM accounts").fetchone()
                t = DB.execute("SELECT COUNT(*) FROM transfers").fetchone()[0]
                return self.send(200, {"accounts": n, "total": total, "transfers": t})
            self.send(404, {"error": "not found"})

    def do_POST(self):
        b = self.body()
        if not isinstance(b, dict):
            b = {}
        with LOCK:
            if self.path == "/accounts":
                owner = b.get("owner")
                if not isinstance(owner, str) or owner.strip() == "":
                    return self.send(422, {"error": "owner is required"})
                if b.get("currency") not in ("EUR", "USD"):
                    return self.send(422, {"error": "currency must be EUR or USD"})
                cur = DB.execute("INSERT INTO accounts (owner, currency, balance) VALUES (?, ?, 0)",
                                 (owner.strip(), b["currency"]))
                DB.commit()
                i = cur.lastrowid
                return self.send(201, account(i), {"Location": "/accounts/{}".format(i)})
            m = re.fullmatch(r"/accounts/([^/]+)/deposits", self.path)
            if m:
                i = path_id(m.group(1))
                if account(i) is None:
                    return self.send(404, {"error": "account not found"})
                if not pos_int(b.get("amount")):
                    return self.send(422, {"error": "amount must be a positive integer"})
                DB.execute("UPDATE accounts SET balance = balance + ? WHERE id = ?", (b["amount"], i))
                DB.commit()
                return self.send(200, {"id": i, "balance": account(i)["balance"]})
            if self.path == "/transfers":
                src, dst, amount = b.get("from"), b.get("to"), b.get("amount")
                if not pos_int(amount):
                    return self.send(422, {"error": "amount must be a positive integer"})
                a = account(src) if pos_int(src) else None
                c = account(dst) if pos_int(dst) else None
                if a is None or c is None:
                    return self.send(404, {"error": "account not found"})
                if src == dst:
                    return self.send(422, {"error": "cannot transfer to the same account"})
                if a["currency"] != c["currency"]:
                    return self.send(409, {"error": "currency mismatch"})
                if a["balance"] < amount:
                    return self.send(409, {"error": "insufficient funds"})
                DB.execute("UPDATE accounts SET balance = balance - ? WHERE id = ?", (amount, src))
                DB.execute("UPDATE accounts SET balance = balance + ? WHERE id = ?", (amount, dst))
                cur = DB.execute("INSERT INTO transfers (src, dst, amount) VALUES (?, ?, ?)",
                                 (src, dst, amount))
                DB.commit()
                return self.send(201, {"id": cur.lastrowid, "from": src, "to": dst, "amount": amount},
                                 {"Location": "/transfers/{}".format(cur.lastrowid)})
            self.send(404, {"error": "not found"})


class S(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


S(("127.0.0.1", 18521), H).serve_forever()
