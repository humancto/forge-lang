import json
import re
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def stats(text):
    words = [w.lower() for w in re.findall(r"[A-Za-z']+", text)]
    counts = {}
    for w in words:
        counts[w] = counts.get(w, 0) + 1
    top = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:3]
    longest = None
    for w in words:
        if longest is None or len(w) > len(longest):
            longest = w
    sentences = sum(1 for seg in re.split(r"[.!?]+", text) if re.search(r"[A-Za-z]", seg))
    return {"words": len(words), "unique": len(counts), "sentences": sentences,
            "top": [{"word": w, "count": c} for w, c in top], "longest": longest}


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
        if self.path == "/analyze":
            text = body.get("text") if isinstance(body, dict) else None
            if not isinstance(text, str):
                return self.send_json({"error": "text must be a string"})
            return self.send_json(stats(text))
        if self.path == "/analyze/batch":
            texts = body.get("texts") if isinstance(body, dict) else None
            if not isinstance(texts, list) or not all(isinstance(t, str) for t in texts):
                return self.send_json({"error": "texts must be an array of strings"})
            results = [stats(t) for t in texts]
            return self.send_json({"results": results,
                                   "total_words": sum(r["words"] for r in results)})
        self.send_json({}, 404)


class Server(ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


Server(("127.0.0.1", 18505), Handler).serve_forever()
