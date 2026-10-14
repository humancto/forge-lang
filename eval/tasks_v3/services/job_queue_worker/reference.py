import json
import re
import socketserver
import sqlite3
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
DB = sqlite3.connect("jobs.db", check_same_thread=False)
DB.execute("CREATE TABLE IF NOT EXISTS jobs (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, "
           "input TEXT NOT NULL, status TEXT NOT NULL, result INTEGER, error TEXT)")
DB.commit()


def worker():
    while True:
        time.sleep(1)
        with LOCK:
            for i, kind, inp in DB.execute("SELECT id, kind, input FROM jobs WHERE status = 'queued' "
                                           "ORDER BY id").fetchall():
                value = json.loads(inp)
                if kind == "sum":
                    DB.execute("UPDATE jobs SET status = 'done', result = ? WHERE id = ?", (sum(value), i))
                elif kind == "words":
                    DB.execute("UPDATE jobs SET status = 'done', result = ? WHERE id = ?",
                               (len(value.split()), i))
                else:
                    DB.execute("UPDATE jobs SET status = 'failed', error = ? WHERE id = ?",
                               ("job asked to fail", i))
            DB.commit()


def job(text):
    if not re.fullmatch(r"[0-9]+", text):
        return None
    r = DB.execute("SELECT id, kind, status, result, error FROM jobs WHERE id = ?", (int(text),)).fetchone()
    if r is None:
        return None
    return {"id": r[0], "kind": r[1], "status": r[2], "result": r[3], "error": r[4]}


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

    def do_GET(self):
        u = urllib.parse.urlsplit(self.path)
        with LOCK:
            if u.path == "/stats":
                out = {"queued": 0, "done": 0, "failed": 0}
                for status, n in DB.execute("SELECT status, COUNT(*) FROM jobs GROUP BY status"):
                    out[status] = n
                return self.send(200, out)
            if u.path == "/jobs":
                q = dict(urllib.parse.parse_qsl(u.query))
                if "status" in q:
                    rows = DB.execute("SELECT id FROM jobs WHERE status = ? ORDER BY id", (q["status"],))
                else:
                    rows = DB.execute("SELECT id FROM jobs ORDER BY id")
                return self.send(200, {"jobs": [r[0] for r in rows]})
            m = re.fullmatch(r"/jobs/([^/]+)", u.path)
            if m:
                j = job(m.group(1))
                return self.send(200, j) if j else self.send(404, {"error": "no such job"})
            self.send(404, {"error": "not found"})

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(n) if n else b""
        try:
            b = json.loads(raw) if raw else {}
        except ValueError:
            b = {}
        if not isinstance(b, dict):
            b = {}
        kind, inp = b.get("kind"), b.get("input")
        if kind not in ("sum", "words", "fail"):
            return self.send(422, {"error": "kind must be sum, words or fail"})
        if kind == "sum" and not (isinstance(inp, list) and all(
                isinstance(x, int) and not isinstance(x, bool) for x in inp)):
            return self.send(422, {"error": "sum needs an array of integers"})
        if kind == "words" and not isinstance(inp, str):
            return self.send(422, {"error": "words needs a string"})
        with LOCK:
            cur = DB.execute("INSERT INTO jobs (kind, input, status) VALUES (?, ?, 'queued')",
                             (kind, json.dumps(inp)))
            DB.commit()
            return self.send(202, {"id": cur.lastrowid, "status": "queued"},
                             {"Location": "/jobs/{}".format(cur.lastrowid)})


class S(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


threading.Thread(target=worker, daemon=True).start()
S(("127.0.0.1", 18525), H).serve_forever()
