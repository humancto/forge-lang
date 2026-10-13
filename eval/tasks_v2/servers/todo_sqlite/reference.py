import json
import re
import socketserver
import sqlite3
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit


def connect():
    conn = sqlite3.connect("todos.db")
    conn.execute("CREATE TABLE IF NOT EXISTS todos (id INTEGER PRIMARY KEY AUTOINCREMENT,"
                 " title TEXT NOT NULL, priority INTEGER NOT NULL, done INTEGER NOT NULL)")
    return conn


def as_todo(row):
    return {"id": row[0], "title": row[1], "priority": row[2], "done": bool(row[3])}


def fetch(conn, tid):
    row = conn.execute("SELECT id, title, priority, done FROM todos WHERE id = ?",
                       (tid,)).fetchone()
    return as_todo(row) if row else None


def clean_title(body):
    t = body.get("title")
    if not isinstance(t, str) or not t.strip():
        return None
    return t.strip()


def todo_id(text):
    return int(text) if re.fullmatch(r"[0-9]+", text) and int(text) > 0 else None


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

    def body(self):
        return json.loads(self.rfile.read(int(self.headers["Content-Length"])))

    def target(self):
        m = re.fullmatch(r"/todos/([^/]+)", urlsplit(self.path).path)
        return todo_id(m.group(1)) if m else None

    def do_POST(self):
        body = self.body()
        title = clean_title(body)
        if title is None:
            return self.send_json({"error": "title is required"})
        prio = body.get("priority", 2)
        if isinstance(prio, bool) or prio not in (1, 2, 3) or not isinstance(prio, int):
            return self.send_json({"error": "priority must be 1, 2 or 3"})
        with connect() as conn:
            cur = conn.execute("INSERT INTO todos (title, priority, done) VALUES (?, ?, 0)",
                               (title, prio))
            self.send_json(fetch(conn, cur.lastrowid))

    def do_GET(self):
        u = urlsplit(self.path)
        with connect() as conn:
            if u.path == "/todos":
                q = {k: v[-1] for k, v in parse_qs(u.query).items()}
                sql = "SELECT id, title, priority, done FROM todos"
                if q.get("done") in ("true", "false"):
                    sql += " WHERE done = %d" % (q["done"] == "true")
                sql += " ORDER BY priority, id" if q.get("sort") == "priority" else " ORDER BY id"
                todos = [as_todo(r) for r in conn.execute(sql)]
                return self.send_json({"count": len(todos), "todos": todos})
            tid = self.target()
            todo = fetch(conn, tid) if tid else None
            self.send_json(todo if todo else {"error": "not found"})

    def do_PUT(self):
        body = self.body()
        with connect() as conn:
            tid = self.target()
            todo = fetch(conn, tid) if tid else None
            if not todo:
                return self.send_json({"error": "not found"})
            if "done" in body and not isinstance(body["done"], bool):
                return self.send_json({"error": "done must be a boolean"})
            if "title" in body:
                title = clean_title(body)
                if title is None:
                    return self.send_json({"error": "title is required"})
                conn.execute("UPDATE todos SET title = ? WHERE id = ?", (title, tid))
            if "done" in body:
                conn.execute("UPDATE todos SET done = ? WHERE id = ?", (int(body["done"]), tid))
            self.send_json(fetch(conn, tid))

    def do_DELETE(self):
        with connect() as conn:
            tid = self.target()
            if not tid or not fetch(conn, tid):
                return self.send_json({"error": "not found"})
            conn.execute("DELETE FROM todos WHERE id = ?", (tid,))
            self.send_json({"deleted": tid})


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18508), Handler).serve_forever()
