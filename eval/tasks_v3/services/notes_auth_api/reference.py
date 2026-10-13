import json
import re
import socketserver
import sqlite3
import threading
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

USERS = {"tok-ada": ("ada", False), "tok-bob": ("bob", False), "tok-cy": ("cy", True)}
LOCK = threading.Lock()
DB = sqlite3.connect("notes.db", check_same_thread=False)
DB.execute("CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY AUTOINCREMENT, owner TEXT, "
           "title TEXT, body TEXT, tags TEXT)")
DB.commit()


def note(row):
    return {"id": row[0], "owner": row[1], "title": row[2], "body": row[3], "tags": json.loads(row[4])}


def all_notes():
    return [note(r) for r in DB.execute("SELECT id, owner, title, body, tags FROM notes ORDER BY id")]


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, obj=None, headers=None):
        data = b"" if obj is None else json.dumps(obj).encode()
        self.send_response(code)
        if obj is not None:
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def user(self):
        auth = self.headers.get("Authorization") or ""
        if auth.startswith("Bearer ") and auth[7:] in USERS:
            return USERS[auth[7:]]
        return None

    def deny(self):
        self.send(401, {"error": "unauthorized"}, {"WWW-Authenticate": 'Bearer realm="notes"'})

    def find(self, text):
        if not re.fullmatch(r"[0-9]+", text):
            return None
        row = DB.execute("SELECT id, owner, title, body, tags FROM notes WHERE id = ?", (int(text),)).fetchone()
        return note(row) if row else None

    def do_GET(self):
        parsed = urllib.parse.urlsplit(self.path)
        u = self.user()
        if u is None:
            return self.deny()
        name, admin = u
        with LOCK:
            if parsed.path == "/me":
                mine = [n for n in all_notes() if n["owner"] == name]
                return self.send(200, {"user": name, "admin": admin, "notes": len(mine)})
            if parsed.path == "/notes":
                q = dict(urllib.parse.parse_qsl(parsed.query))
                limit, offset = q.get("limit", "10"), q.get("offset", "0")
                if not re.fullmatch(r"[0-9]+", limit) or not 1 <= int(limit) <= 50:
                    return self.send(400, {"error": "invalid limit"})
                if not re.fullmatch(r"[0-9]+", offset):
                    return self.send(400, {"error": "invalid offset"})
                notes = [n for n in all_notes() if admin or n["owner"] == name]
                if "tag" in q:
                    notes = [n for n in notes if q["tag"] in n["tags"]]
                page = notes[int(offset):int(offset) + int(limit)]
                return self.send(200, {"notes": page}, {"X-Total-Count": str(len(notes))})
            m = re.fullmatch(r"/notes/([^/]+)", parsed.path)
            if m:
                n = self.find(m.group(1))
                if n is None:
                    return self.send(404, {"error": "not found"})
                if not admin and n["owner"] != name:
                    return self.send(403, {"error": "forbidden"})
                return self.send(200, n)
            self.send(404, {"error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        u = self.user()
        if u is None:
            return self.deny()
        try:
            b = json.loads(raw) if raw else {}
        except ValueError:
            b = {}
        if not isinstance(b, dict):
            b = {}
        with LOCK:
            if self.path != "/notes":
                return self.send(404, {"error": "not found"})
            title = b.get("title")
            if not isinstance(title, str) or not 1 <= len(title.strip()) <= 80:
                return self.send(422, {"error": "title must be 1-80 characters"})
            body = b.get("body", "")
            if not isinstance(body, str):
                return self.send(422, {"error": "body must be a string"})
            tags = b.get("tags", [])
            if not isinstance(tags, list) or len(tags) > 5:
                return self.send(422, {"error": "tags must be a list of up to 5 tags"})
            clean = []
            for t in tags:
                if not isinstance(t, str) or not re.fullmatch(r"[a-z0-9-]{1,20}", t):
                    return self.send(422, {"error": "invalid tag: {}".format(t if isinstance(t, str) else json.dumps(t))})
                if t not in clean:
                    clean.append(t)
            cur = DB.execute("INSERT INTO notes (owner, title, body, tags) VALUES (?, ?, ?, ?)",
                             (u[0], title.strip(), body, json.dumps(clean)))
            DB.commit()
            n = self.find(str(cur.lastrowid))
            return self.send(201, n, {"Location": "/notes/{}".format(n["id"])})

    def do_DELETE(self):
        u = self.user()
        if u is None:
            return self.deny()
        with LOCK:
            m = re.fullmatch(r"/notes/([^/]+)", self.path)
            n = self.find(m.group(1)) if m else None
            if n is None:
                return self.send(404, {"error": "not found"})
            if not u[1] and n["owner"] != u[0]:
                return self.send(403, {"error": "forbidden"})
            DB.execute("DELETE FROM notes WHERE id = ?", (n["id"],))
            DB.commit()
            return self.send(204)


class S(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


S(("127.0.0.1", 18522), H).serve_forever()
