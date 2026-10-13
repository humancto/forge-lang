import hashlib
import json
import re
import socketserver
import sqlite3
import threading
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
DB = sqlite3.connect("kv.db", check_same_thread=False)
DB.execute("CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL, version INTEGER NOT NULL)")
DB.commit()


def etag(value):
    return '"' + hashlib.sha256(value.encode("utf-8")).hexdigest()[:16] + '"'


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, body=None, headers=None, text=False):
        data = b""
        self.send_response(code)
        if body is not None:
            data = body.encode("utf-8") if text else json.dumps(body).encode("utf-8")
            self.send_header("Content-Type", "text/plain; charset=utf-8" if text else "application/json")
        self.send_header("Content-Length", str(len(data)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def key(self):
        parsed = urllib.parse.urlsplit(self.path)
        m = re.fullmatch(r"/kv/([^/]+)", parsed.path)
        if not m:
            return None, None
        k = urllib.parse.unquote(m.group(1))
        return k, re.fullmatch(r"[A-Za-z0-9 _.-]{1,64}", k) is not None

    def current(self, k):
        return DB.execute("SELECT v, version FROM kv WHERE k = ?", (k,)).fetchone()

    def do_GET(self):
        parsed = urllib.parse.urlsplit(self.path)
        with LOCK:
            if parsed.path == "/kv":
                prefix = dict(urllib.parse.parse_qsl(parsed.query)).get("prefix", "")
                keys = sorted(r[0] for r in DB.execute("SELECT k FROM kv") if r[0].startswith(prefix))
                return self.send(200, {"keys": keys, "count": len(keys)})
            k, ok = self.key()
            if k is None:
                return self.send(404, {"error": "not found"})
            if not ok:
                return self.send(400, {"error": "bad key"})
            row = self.current(k)
            if row is None:
                return self.send(404, {"error": "no such key"})
            tag = etag(row[0])
            if self.headers.get("If-None-Match") == tag:
                return self.send(304, None, {"ETag": tag})
            return self.send(200, row[0], {"ETag": tag}, text=True)

    def do_PUT(self):
        n = int(self.headers.get("Content-Length") or 0)
        value = (self.rfile.read(n) if n else b"").decode("utf-8")
        with LOCK:
            k, ok = self.key()
            if k is None:
                return self.send(404, {"error": "not found"})
            if not ok:
                return self.send(400, {"error": "bad key"})
            row = self.current(k)
            if_match = self.headers.get("If-Match")
            if if_match is not None and (row is None or etag(row[0]) != if_match):
                return self.send(412, {"error": "precondition failed"})
            if self.headers.get("If-None-Match") == "*" and row is not None:
                return self.send(412, {"error": "precondition failed"})
            version = 1 if row is None else row[1] + 1
            DB.execute("INSERT OR REPLACE INTO kv (k, v, version) VALUES (?, ?, ?)", (k, value, version))
            DB.commit()
            return self.send(201 if row is None else 200, {"key": k, "version": version},
                             {"ETag": etag(value)})

    def do_DELETE(self):
        with LOCK:
            k, ok = self.key()
            if k is None:
                return self.send(404, {"error": "not found"})
            if not ok:
                return self.send(400, {"error": "bad key"})
            if self.current(k) is None:
                return self.send(404, {"error": "no such key"})
            DB.execute("DELETE FROM kv WHERE k = ?", (k,))
            DB.commit()
            return self.send(204)


class S(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


S(("127.0.0.1", 18523), H).serve_forever()
