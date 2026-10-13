import hashlib
import json
import socketserver
import sqlite3
import threading
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SECRET = "whsec-42"
LOCK = threading.Lock()
DB = sqlite3.connect("inbox.db", check_same_thread=False)
DB.execute("CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY AUTOINCREMENT, source TEXT, "
           "delivery TEXT, event TEXT, summary TEXT)")
DB.commit()


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, obj, text=None):
        data = (text if text is not None else json.dumps(obj)).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "text/plain; charset=utf-8" if text is not None
                         else "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def store(self, source, delivery, event, summary):
        cur = DB.execute("INSERT INTO events (source, delivery, event, summary) VALUES (?, ?, ?, ?)",
                         (source, delivery, event, summary))
        DB.commit()
        return cur.lastrowid

    def do_GET(self):
        u = urllib.parse.urlsplit(self.path)
        if u.path != "/events":
            return self.send(404, {"error": "not found"})
        q = dict(urllib.parse.parse_qsl(u.query))
        with LOCK:
            rows = DB.execute("SELECT source, event, summary FROM events ORDER BY id").fetchall()
        events = [{"source": s, "event": e, "summary": m} for s, e, m in rows
                  if "source" not in q or s == q["source"]]
        self.send(200, {"events": events, "count": len(events)})

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        raw = (self.rfile.read(n) if n else b"").decode("utf-8")
        ctype = (self.headers.get("Content-Type") or "").split(";")[0].strip().lower()
        with LOCK:
            if self.path == "/hooks/github":
                delivery = self.headers.get("X-Delivery")
                if not delivery:
                    return self.send(400, {"error": "missing delivery id"})
                want = "sha256=" + hashlib.sha256((SECRET + raw).encode("utf-8")).hexdigest()
                if self.headers.get("X-Signature") != want:
                    return self.send(401, {"error": "bad signature"})
                if DB.execute("SELECT 1 FROM events WHERE delivery = ?", (delivery,)).fetchone():
                    return self.send(200, {"duplicate": True, "delivery": delivery})
                try:
                    body = json.loads(raw)
                except ValueError:
                    body = None
                if not isinstance(body, dict) or not isinstance(body.get("event"), str):
                    return self.send(422, {"error": "event is required"})
                sender = body.get("sender")
                login = sender.get("login") if isinstance(sender, dict) else None
                who = login if isinstance(login, str) else "unknown"
                self.store("github", delivery, body["event"], "{} by {}".format(body["event"], who))
                return self.send(202, {"accepted": delivery})
            if self.path == "/hooks/form":
                if ctype != "application/x-www-form-urlencoded":
                    return self.send(415, {"error": "expected a form"})
                form = dict(urllib.parse.parse_qsl(raw, keep_blank_values=True))
                name, email = form.get("name", "").strip(), form.get("email", "").strip()
                if not name or "@" not in email:
                    return self.send(422, {"error": "name and email are required"})
                i = self.store("form", None, "form", "{} <{}>".format(name, email))
                return self.send(202, {"accepted": "form-{}".format(i)})
            if self.path == "/hooks/text":
                lines = [l.strip() for l in raw.split("\n") if l.strip()]
                for l in lines:
                    self.store("text", None, "line", l)
                return self.send(202, None, text="accepted {} lines".format(len(lines)))
            self.send(404, {"error": "not found"})


class S(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


S(("127.0.0.1", 18524), H).serve_forever()
