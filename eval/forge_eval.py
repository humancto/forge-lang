#!/usr/bin/env python3
"""Forge evaluation harness (RFC 0008, section 1).

Measures how often a model writes a correct Forge program on the first try
(pass@1), compared with Python on the same tasks, and classifies every
failure so the next language fix is chosen from data.

Commands (see eval/README.md):

  validate     run every reference solution (Forge on vm + interp and under
               --edition 2027, Python) and check the expected output
  generate     ask a provider (anthropic | command | dir) for solutions
  score        run the solutions of a run in the sandbox and write
               report.json + report.md
  compare      side-by-side table of two or more scored runs
  blind-pack   write Markdown packs of task prompts for agents acting as
               models (no API key needed)

Python 3.9+, standard library only.
"""

import argparse
import concurrent.futures
import datetime
import http.server
import json
import os
import random
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

EVAL_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(EVAL_DIR)
TASKS_DIR = os.path.join(EVAL_DIR, "tasks")
LLMS_TXT = os.path.join(REPO_ROOT, "llms.txt")

LANGS = {
    "forge": {"ext": "fg", "name": "Forge", "fence": "forge"},
    "python": {"ext": "py", "name": "Python", "fence": "python"},
}
CATEGORIES = [
    "basics",
    "strings",
    "collections",
    "structs_adts",
    "errors",
    "json_csv",
    "functional",
    "algorithms",
    "concurrency",
    "http",
    "files",
]
CAPABILITIES = {"read", "write", "net", "env", "run"}
CONTEXTS = ["none", "llms", "llms+errors"]
FAILURE_CLASSES = [
    "syntax_error",
    "type_error",
    "runtime_error",
    "wrong_output",
    "timeout",
    "no_code",
]
DEFAULT_MODEL = "claude-sonnet-5-5"
DEFAULT_TIMEOUT = 10

# Natural-syntax keywords and joke builtins that edition 2027 removes
# (RFC 0008 section 5). Reference solutions must not use them.
FORBIDDEN_IN_REFERENCES = [
    r"^\s*set\s+(mut\s+)?\w+\s+to\b",
    r"^\s*change\s+\w+\s+to\b",
    r"^\s*define\s+\w+\s*\(",
    r"\b(otherwise|nah)\s*\{",
    r"\bunpack\s*[\[{]",
    r"\brepeat\s+\S+\s+times\b",
    r"\b(say|yell|whisper|sus|bruh|bet|no_cap|ick|cook|yolo|ghost|slay)\s*\(",
    r"^\s*(say|yell|whisper)\s+",
    r"\bmust\s+",
    r"^\s*safe\s*\{",
    r"^\s*check\s+\w+\s+is\b",
    r"\b(hold|emit)\s+",
]


# ---------------------------------------------------------------------------
# TOML (tomllib on 3.11+, a small subset parser otherwise)
# ---------------------------------------------------------------------------


class MiniToml:
    """Parser for the TOML subset task files use: comments, `key = value`,
    `[table]`, `[[array.of.tables]]`, basic/literal/multi-line strings,
    integers, floats, booleans, (multi-line) arrays and inline tables."""

    def __init__(self, text):
        self.s = text
        self.i = 0

    def error(self, msg):
        line = self.s.count("\n", 0, self.i) + 1
        raise ValueError("TOML line {}: {}".format(line, msg))

    def parse(self):
        root = {}
        current = root
        while True:
            self.skip_ws_comments(newlines=True)
            if self.i >= len(self.s):
                return root
            if self.s.startswith("[[", self.i):
                self.i += 2
                keys = self.parse_key_path("]]")
                parent = self.descend(root, keys[:-1])
                arr = parent.setdefault(keys[-1], [])
                if not isinstance(arr, list):
                    self.error("{} is not an array of tables".format(keys[-1]))
                current = {}
                arr.append(current)
            elif self.s[self.i] == "[":
                self.i += 1
                keys = self.parse_key_path("]")
                current = self.descend(root, keys)
            else:
                keys = self.parse_key_path("=")
                self.skip_ws_comments(newlines=False)
                value = self.parse_value()
                target = self.descend(current, keys[:-1])
                if keys[-1] in target:
                    self.error("duplicate key {}".format(keys[-1]))
                target[keys[-1]] = value
                self.skip_ws_comments(newlines=False)
                if self.i < len(self.s) and self.s[self.i] != "\n":
                    self.error("expected end of line")

    def descend(self, table, keys):
        for k in keys:
            nxt = table.setdefault(k, {})
            if isinstance(nxt, list):
                nxt = nxt[-1]
            table = nxt
        return table

    def skip_ws_comments(self, newlines):
        while self.i < len(self.s):
            c = self.s[self.i]
            if c in " \t\r" or (newlines and c == "\n"):
                self.i += 1
            elif c == "#":
                while self.i < len(self.s) and self.s[self.i] != "\n":
                    self.i += 1
            else:
                break

    def parse_key_path(self, terminator):
        keys = []
        while True:
            self.skip_ws_comments(newlines=False)
            if self.s[self.i] in "\"'":
                keys.append(self.parse_string())
            else:
                m = re.compile(r"[A-Za-z0-9_-]+").match(self.s, self.i)
                if not m:
                    self.error("expected a key")
                keys.append(m.group(0))
                self.i = m.end()
            self.skip_ws_comments(newlines=False)
            if self.s.startswith(terminator, self.i):
                self.i += len(terminator)
                return keys
            if self.s[self.i] == ".":
                self.i += 1
                continue
            self.error("expected {!r}".format(terminator))

    def parse_value(self):
        s = self.s
        c = s[self.i] if self.i < len(s) else ""
        if c in "\"'":
            return self.parse_string()
        if c == "[":
            self.i += 1
            items = []
            while True:
                self.skip_ws_comments(newlines=True)
                if s[self.i] == "]":
                    self.i += 1
                    return items
                items.append(self.parse_value())
                self.skip_ws_comments(newlines=True)
                if s[self.i] == ",":
                    self.i += 1
                elif s[self.i] != "]":
                    self.error("expected , or ] in array")
        if c == "{":
            self.i += 1
            table = {}
            self.skip_ws_comments(newlines=False)
            if s[self.i] == "}":
                self.i += 1
                return table
            while True:
                keys = self.parse_key_path("=")
                self.skip_ws_comments(newlines=False)
                self.descend(table, keys[:-1])[keys[-1]] = self.parse_value()
                self.skip_ws_comments(newlines=False)
                if s[self.i] == ",":
                    self.i += 1
                elif s[self.i] == "}":
                    self.i += 1
                    return table
                else:
                    self.error("expected , or } in inline table")
        for word, value in (("true", True), ("false", False)):
            if s.startswith(word, self.i):
                self.i += len(word)
                return value
        m = re.compile(r"[+-]?(\d[\d_]*)(\.\d[\d_]*)?([eE][+-]?\d+)?").match(s, self.i)
        if not m:
            self.error("expected a value")
        self.i = m.end()
        text = m.group(0).replace("_", "")
        if m.group(2) or m.group(3):
            return float(text)
        return int(text)

    def parse_string(self):
        s = self.s
        for quote, literal in (('"""', False), ("'''", True)):
            if s.startswith(quote, self.i):
                self.i += 3
                if s.startswith("\r\n", self.i):
                    self.i += 2
                elif s.startswith("\n", self.i):
                    self.i += 1
                end = s.find(quote, self.i)
                if end < 0:
                    self.error("unterminated multi-line string")
                # A closing run of up to two extra quotes belongs to the string.
                while s.startswith(quote[0], end + 3):
                    end += 1
                raw = s[self.i:end]
                self.i = end + 3
                if literal:
                    return raw
                raw = re.sub(r"\\\r?\n\s*", "", raw)
                return self.unescape(raw)
        quote = s[self.i]
        self.i += 1
        start = self.i
        if quote == "'":
            end = s.find("'", start)
            if end < 0 or "\n" in s[start:end]:
                self.error("unterminated string")
            self.i = end + 1
            return s[start:end]
        out = []
        while True:
            if self.i >= len(s) or s[self.i] == "\n":
                self.error("unterminated string")
            c = s[self.i]
            if c == "\\":
                out.append(s[self.i:self.i + 2])
                if s[self.i + 1] == "u":
                    out.append(s[self.i + 2:self.i + 6])
                    self.i += 4
                elif s[self.i + 1] == "U":
                    out.append(s[self.i + 2:self.i + 10])
                    self.i += 8
                self.i += 2
            elif c == '"':
                self.i += 1
                return self.unescape("".join(out))
            else:
                out.append(c)
                self.i += 1

    def unescape(self, raw):
        simple = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\", "b": "\b", "f": "\f"}

        def repl(m):
            body = m.group(1)
            if body[0] in "uU":
                return chr(int(body[1:], 16))
            return simple[body]

        return re.sub(r'\\(u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8}|[ntr"\\bf])', repl, raw)


def load_toml(path, force_mini=False):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    if not force_mini:
        try:
            import tomllib  # type: ignore

            return tomllib.loads(text)
        except ImportError:
            pass
    return MiniToml(text).parse()


# ---------------------------------------------------------------------------
# Tasks
# ---------------------------------------------------------------------------


class Task:
    def __init__(self, path, data):
        self.dir = path
        self.data = data
        self.id = data.get("id", "")
        self.category = data.get("category", "")
        self.title = data.get("title", "")
        self.prompt = data.get("prompt", "").strip()
        self.stdin = data.get("stdin")
        self.args = list(data.get("args", []))
        self.expected = data.get("expected_stdout", "")
        self.needs = list(data.get("needs", []))
        self.timeout = float(data.get("timeout", DEFAULT_TIMEOUT))
        self.mode = data.get("mode", "run")
        self.output_files = list(data.get("output_files", []))
        self.requests = list(data.get("request", []))
        self.port = data.get("port")
        self.trim = bool(data.get("trim", True))
        self.sort_lines = bool(data.get("sort_lines", False))
        self.float_tolerance = data.get("float_tolerance")

    def reference(self, lang):
        return os.path.join(self.dir, "reference." + LANGS[lang]["ext"])

    def files_dir(self):
        return os.path.join(self.dir, "files")

    def www_dir(self):
        return os.path.join(self.dir, "www")

    def problems(self):
        """Schema problems with this task (empty when valid)."""
        out = []
        for key in ("id", "category", "title", "prompt", "expected_stdout"):
            if key not in self.data:
                out.append("missing required key `{}`".format(key))
        if self.id != os.path.basename(self.dir):
            out.append("id `{}` does not match directory name".format(self.id))
        if self.category != os.path.basename(os.path.dirname(self.dir)):
            out.append("category `{}` does not match parent directory".format(self.category))
        if self.category not in CATEGORIES:
            out.append("unknown category `{}`".format(self.category))
        bad = set(self.needs) - CAPABILITIES
        if bad:
            out.append("unknown needs: {}".format(sorted(bad)))
        if self.mode not in ("run", "server"):
            out.append("mode must be run or server")
        if self.mode == "server" and not self.requests:
            out.append("server tasks need [[request]] entries")
        if self.mode == "server" and "net" not in self.needs:
            out.append("server tasks need `net`")
        uses_http = "{base_url}" in json.dumps([self.args, self.stdin])
        if uses_http and "net" not in self.needs:
            out.append("tasks using {base_url} need `net`")
        for lang in LANGS:
            if not os.path.isfile(self.reference(lang)):
                out.append("missing reference.{}".format(LANGS[lang]["ext"]))
        unknown = set(self.data) - {
            "id", "category", "title", "prompt", "stdin", "args", "expected_stdout",
            "needs", "timeout", "mode", "output_files", "request", "trim",
            "sort_lines", "float_tolerance", "notes", "tags", "port",
        }
        if unknown:
            out.append("unknown keys: {}".format(sorted(unknown)))
        return out


def load_tasks(selector=None):
    tasks = []
    for category in sorted(os.listdir(TASKS_DIR)):
        cdir = os.path.join(TASKS_DIR, category)
        if not os.path.isdir(cdir):
            continue
        for tid in sorted(os.listdir(cdir)):
            tdir = os.path.join(cdir, tid)
            toml_path = os.path.join(tdir, "task.toml")
            if os.path.isfile(toml_path):
                tasks.append(Task(tdir, load_toml(toml_path)))
    if selector:
        tasks = [t for t in tasks if selector(t)]
    return tasks


def task_selector(args):
    ids = set(filter(None, (getattr(args, "tasks", None) or "").split(",")))
    cats = set(filter(None, (getattr(args, "category", None) or "").split(",")))

    def sel(t):
        if ids and t.id not in ids:
            return False
        if cats and t.category not in cats:
            return False
        return True

    return sel


# ---------------------------------------------------------------------------
# Local HTTP fixture (for `http` tasks that act as clients)
# ---------------------------------------------------------------------------


def find_task_dir(task_id):
    """The directory of the task with this id (ids are directory names)."""
    if not re.fullmatch(r"[A-Za-z0-9_-]+", task_id):
        return None
    for category in os.listdir(TASKS_DIR):
        tdir = os.path.join(TASKS_DIR, category, task_id)
        if os.path.isfile(os.path.join(tdir, "task.toml")):
            return tdir
    return None


class _FixtureHandler(http.server.BaseHTTPRequestHandler):
    """Routes, all under /t/<task_id>/:
       <file>          a file from the task's www/ directory
       echo            JSON {method, path, query, body, content_type}
       status/<code>   that status with body {"status": code}"""

    server_version = "ForgeEvalFixture/1"

    def log_message(self, *args):
        pass

    def _send(self, status, body, ctype="application/json"):
        data = body.encode("utf-8") if isinstance(body, str) else body
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _handle(self):
        parsed = urllib.parse.urlsplit(self.path)
        parts = parsed.path.strip("/").split("/")
        if len(parts) < 3 or parts[0] != "t":
            return self._send(404, json.dumps({"error": "not found"}))
        task = find_task_dir(parts[1])
        if task is None:
            return self._send(404, json.dumps({"error": "unknown task"}))
        rest = parts[2:]
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        if rest == ["echo"]:
            text = raw.decode("utf-8", "replace")
            try:
                body = json.loads(text) if text else None
            except ValueError:
                body = text
            query = dict(urllib.parse.parse_qsl(parsed.query))
            return self._send(200, json.dumps({
                "method": self.command,
                "path": "/" + "/".join(rest),
                "query": query,
                "body": body,
                "content_type": self.headers.get("Content-Type", ""),
            }))
        if len(rest) == 2 and rest[0] == "status" and rest[1].isdigit():
            code = int(rest[1])
            return self._send(code, json.dumps({"status": code}))
        if self.command != "GET":
            return self._send(405, json.dumps({"error": "method not allowed"}))
        www = os.path.join(task, "www")
        path = os.path.normpath(os.path.join(www, *rest))
        if not path.startswith(www + os.sep) or not os.path.isfile(path):
            return self._send(404, json.dumps({"error": "not found"}))
        ctype = {
            ".json": "application/json",
            ".csv": "text/csv",
            ".txt": "text/plain",
            ".html": "text/html",
        }.get(os.path.splitext(path)[1], "application/octet-stream")
        with open(path, "rb") as f:
            return self._send(200, f.read(), ctype)

    do_GET = do_POST = do_PUT = do_DELETE = do_PATCH = _handle


class Fixture:
    _lock = threading.Lock()
    _server = None

    @classmethod
    def base_url(cls, task):
        with cls._lock:
            if cls._server is None:
                srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _FixtureHandler)
                srv.daemon_threads = True
                threading.Thread(target=srv.serve_forever, daemon=True).start()
                cls._server = srv
            port = cls._server.server_address[1]
        return "http://127.0.0.1:{}/t/{}".format(port, task.id)


_PORT_LOCKS = {}
_PORT_LOCKS_GUARD = threading.Lock()


def port_lock(port):
    """Serialises runs that share a fixed port within this process."""
    with _PORT_LOCKS_GUARD:
        return _PORT_LOCKS.setdefault(port, threading.Lock())


def wait_port_free(port, timeout=10.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("127.0.0.1", port))
                return True
            except OSError:
                time.sleep(0.1)
    return False


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---------------------------------------------------------------------------
# Running one program
# ---------------------------------------------------------------------------


class RunConfig:
    def __init__(self, lang, forge="forge", engine="vm", edition=None, python="python3",
                 grant_stdin_file=False):
        self.lang = lang
        # Counterfactual scoring: also let the program read /dev/stdin as a
        # file, to see which failures hide behind that one cause.
        self.grant_stdin_file = grant_stdin_file
        self.forge = forge
        self.engine = engine
        self.edition = edition
        self.python = python

    def label(self):
        if self.lang == "python":
            return "python"
        return "forge-{}{}".format(self.engine, "-" + self.edition if self.edition else "")


def substitute(value, subs):
    if isinstance(value, str):
        for k, v in subs.items():
            value = value.replace("{" + k + "}", v)
    return value


def forge_command(cfg, task, workdir, source, extra_args, port=None):
    cmd = [cfg.forge]
    if cfg.engine == "interp":
        cmd.append("--interp")
    if cfg.edition:
        cmd += ["--edition", cfg.edition]
    cmd += ["--sandbox", "--error-format", "json", "--max-time", str(task.timeout)]
    needs = set(task.needs)
    reads = [workdir] if "read" in needs else []
    if cfg.grant_stdin_file:
        reads.append("/dev/stdin")
    if reads:
        cmd.append("--allow-read=" + ",".join(reads))
    if "write" in needs:
        cmd.append("--allow-write=" + workdir)
    if "net" in needs:
        cmd.append("--allow-net=127.0.0.1,localhost")
    if "env" in needs or task.mode == "server":
        cmd.append("--allow-env")
    if "run" in needs:
        cmd.append("--allow-run")
    cmd += ["run", source] + extra_args
    return cmd


def run_program(cfg, task, source_path):
    """Run a solution; return a dict with stdout, stderr, returncode,
    timed_out, duration, and the transcript compared with expected_stdout."""
    workdir = tempfile.mkdtemp(prefix="forge-eval-")
    try:
        if os.path.isdir(task.files_dir()):
            for name in os.listdir(task.files_dir()):
                src = os.path.join(task.files_dir(), name)
                dst = os.path.join(workdir, name)
                if os.path.isdir(src):
                    shutil.copytree(src, dst)
                else:
                    shutil.copy(src, dst)
        ext = LANGS[cfg.lang]["ext"]
        source = os.path.join(workdir, "main." + ext)
        shutil.copy(source_path, source)
        subs = {"workdir": workdir}
        if "{base_url}" in json.dumps([task.args, task.stdin]):
            subs["base_url"] = Fixture.base_url(task)
        port = None
        if task.mode == "server":
            # A fixed port in the prompt (Forge's @server takes a literal
            # port), else a free one passed in $PORT.
            port = int(task.port) if task.port else free_port()
            subs["port"] = str(port)
        args = [substitute(a, subs) for a in task.args]
        stdin = substitute(task.stdin, subs) if task.stdin is not None else None
        env = dict(os.environ)
        env["FORGE_HTTP_ALLOW_PRIVATE"] = "1"
        env["NO_COLOR"] = "1"
        env["PYTHONIOENCODING"] = "utf-8"
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        if port:
            env["PORT"] = str(port)
        if cfg.lang == "forge":
            cmd = forge_command(cfg, task, workdir, source, args, port)
        else:
            cmd = [cfg.python, source] + args
        start = time.time()
        if task.mode == "server":
            with port_lock(port):
                result = run_server(cmd, workdir, env, task, port)
        else:
            result = run_process(cmd, workdir, env, stdin, task.timeout + 5)
        result["duration"] = round(time.time() - start, 3)
        transcript = result["stdout"]
        for name in task.output_files:
            path = os.path.join(workdir, name)
            content = "<missing>"
            if os.path.isfile(path):
                with open(path, encoding="utf-8", errors="replace") as f:
                    content = f.read()
            if transcript and not transcript.endswith("\n"):
                transcript += "\n"
            transcript += "--- {} ---\n{}".format(name, content)
        result["transcript"] = transcript
        result["command"] = cmd
        return result
    finally:
        shutil.rmtree(workdir, ignore_errors=True)


def run_process(cmd, cwd, env, stdin, timeout):
    try:
        p = subprocess.run(
            cmd,
            cwd=cwd,
            env=env,
            input=(stdin if stdin is not None else "").encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
        return {
            "stdout": p.stdout.decode("utf-8", "replace"),
            "stderr": p.stderr.decode("utf-8", "replace"),
            "returncode": p.returncode,
            "timed_out": False,
        }
    except subprocess.TimeoutExpired as e:
        return {
            "stdout": (e.stdout or b"").decode("utf-8", "replace"),
            "stderr": (e.stderr or b"").decode("utf-8", "replace"),
            "returncode": None,
            "timed_out": True,
        }
    except OSError as e:
        return {"stdout": "", "stderr": str(e), "returncode": 127, "timed_out": False}


def http_request(method, url, body=None, timeout=5):
    data = None
    headers = {}
    if body is not None:
        data = body.encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")


def canonical_body(text):
    try:
        return json.dumps(json.loads(text), sort_keys=True, separators=(", ", ": "))
    except ValueError:
        return text.strip()


def run_server(cmd, cwd, env, task, port):
    """Start a server program, wait for its port, send the task's requests
    and build a transcript `STATUS <code>\\n<canonical body>` per request."""
    wait_port_free(port)
    proc = subprocess.Popen(
        cmd, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    out_lines = []
    timed_out = False
    try:
        deadline = time.time() + task.timeout
        up = False
        while time.time() < deadline and proc.poll() is None:
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                    up = True
                    break
            except OSError:
                time.sleep(0.05)
        if up:
            base = "http://127.0.0.1:{}".format(port)
            for req in task.requests:
                try:
                    status, body = http_request(
                        req.get("method", "GET"), base + req["path"], req.get("body"))
                    out_lines.append("STATUS {}\n{}".format(status, canonical_body(body)))
                except Exception as e:  # connection reset, timeout...
                    out_lines.append("ERROR {}".format(type(e).__name__))
        elif proc.poll() is None:
            timed_out = True
    finally:
        if proc.poll() is None:
            proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
    rc = proc.returncode
    exited_early = not out_lines and not timed_out
    return {
        "stdout": "\n".join(out_lines) + ("\n" if out_lines else ""),
        "stderr": stderr.decode("utf-8", "replace"),
        "program_stdout": stdout.decode("utf-8", "replace"),
        # A server that exits on its own before answering failed; one we
        # terminated after the requests is a success.
        "returncode": rc if exited_early else 0,
        "timed_out": timed_out,
    }


# ---------------------------------------------------------------------------
# Comparing and classifying
# ---------------------------------------------------------------------------

NUMBER = re.compile(r"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$")


def normalize(text, task):
    text = text.replace("\r\n", "\n")
    lines = text.split("\n")
    if task.trim:
        lines = [line.rstrip() for line in lines]
        while lines and lines[-1] == "":
            lines.pop()
        while lines and lines[0] == "":
            lines.pop(0)
    if task.sort_lines:
        lines = sorted(lines)
    return lines


def outputs_match(actual, task):
    exp = normalize(task.expected, task)
    got = normalize(actual, task)
    if task.float_tolerance is None:
        return exp == got
    if len(exp) != len(got):
        return False
    tol = float(task.float_tolerance)
    for a, b in zip(exp, got):
        ta, tb = a.split(), b.split()
        if len(ta) != len(tb):
            return False
        for x, y in zip(ta, tb):
            if x == y:
                continue
            if NUMBER.match(x) and NUMBER.match(y) and abs(float(x) - float(y)) <= tol:
                continue
            return False
    return True


def parse_forge_diagnostics(stderr):
    diags = []
    for line in stderr.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if isinstance(d, dict) and "code" in d and "message" in d:
            diags.append(d)
    return diags


PY_EXC = re.compile(r"^(\w+(?:\.\w+)*(?:Error|Exception|Exit|Interrupt|Warning)|\w+Error)(?::\s*(.*))?$")


def classify(lang, result, passed):
    """Return (class, code, message, warnings)."""
    if passed:
        return ("pass", None, None, [])
    stderr = result.get("stderr", "")
    if result.get("timed_out") or (lang == "forge" and result.get("returncode") == 124):
        return ("timeout", None, "exceeded the time limit", [])
    if lang == "forge":
        diags = parse_forge_diagnostics(stderr)
        errors = [d for d in diags if d.get("severity") == "error"]
        warnings = sorted({d["code"] for d in diags if d.get("severity") != "error"})
        if errors:
            d = errors[0]
            klass = {"syntax": "syntax_error", "type": "type_error"}.get(
                d.get("phase"), "runtime_error")
            return (klass, d.get("code"), d.get("message", ""), warnings)
        if result.get("returncode") not in (0, None):
            lines = [l for l in stderr.strip().splitlines() if l.strip()]
            msg = lines[-1] if lines else "exit code {}".format(result.get("returncode"))
            if "fuel exhausted" in msg or "memory limit" in msg:
                return ("timeout", None, msg, warnings)
            return ("runtime_error", "exit", msg, warnings)
        return ("wrong_output", None, None, warnings)
    # python
    if result.get("returncode") not in (0, None):
        lines = [l for l in stderr.strip().splitlines() if l.strip()]
        for line in reversed(lines):
            m = PY_EXC.match(line.strip())
            if m:
                name = m.group(1).split(".")[-1]
                klass = "syntax_error" if name in (
                    "SyntaxError", "IndentationError", "TabError") else "runtime_error"
                return (klass, name, (m.group(2) or "").strip(), [])
        msg = lines[-1] if lines else "exit code {}".format(result.get("returncode"))
        return ("runtime_error", "exit", msg, [])
    return ("wrong_output", None, None, [])


def message_prefix(message, n=60):
    """Group similar messages: quoted names and numbers become placeholders."""
    if not message:
        return ""
    m = message.splitlines()[0]
    m = re.sub(r"'[^']*'", "'_'", m)
    m = re.sub(r'"[^"]*"', '"_"', m)
    m = re.sub(r"`[^`]*`", "`_`", m)
    m = re.sub(r"\b\d+(\.\d+)?\b", "N", m)
    return m[:n]


def diff_snippet(expected, actual, task, limit=6):
    exp = normalize(expected, task)
    got = normalize(actual, task)
    out = []
    for i in range(max(len(exp), len(got))):
        e = exp[i] if i < len(exp) else "<none>"
        g = got[i] if i < len(got) else "<none>"
        if e != g:
            out.append("line {}: expected {!r}, got {!r}".format(i + 1, e, g))
            if len(out) >= limit:
                break
    return out


def evaluate(cfg, task, source_path):
    """Run + compare + classify one solution."""
    if not source_path or not os.path.isfile(source_path) or not open(
            source_path, encoding="utf-8", errors="replace").read().strip():
        return {"status": "fail", "class": "no_code", "code": None,
                "message": "no solution", "warnings": [], "duration": 0}
    result = run_program(cfg, task, source_path)
    passed = (result.get("returncode") == 0 and not result.get("timed_out")
              and outputs_match(result["transcript"], task))
    klass, code, message, warnings = classify(cfg.lang, result, passed)
    out = {
        "status": "pass" if passed else "fail",
        "class": klass,
        "code": code,
        "message": message,
        "warnings": warnings,
        "duration": result["duration"],
        "stdout": result["transcript"][-4000:],
        "stderr": result["stderr"][-4000:],
    }
    if klass == "wrong_output":
        out["diff"] = diff_snippet(task.expected, result["transcript"], task)
    return out


# ---------------------------------------------------------------------------
# validate
# ---------------------------------------------------------------------------


def find_forge(arg):
    candidates = [arg] if arg else []
    if os.environ.get("FORGE_BIN"):
        candidates.append(os.environ["FORGE_BIN"])
    candidates += [
        os.path.join(REPO_ROOT, "target", "debug", "forge"),
        os.path.join(REPO_ROOT, "target", "release", "forge"),
    ]
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    found = shutil.which("forge")
    if found:
        return found
    sys.exit("forge binary not found: pass --forge PATH or set FORGE_BIN")


def strip_strings(line):
    """The code part of a Forge line: string literals blanked, comment cut."""
    line = re.sub(r'"(?:[^"\\]|\\.)*"', '""', line)
    return line.split("//")[0]


def lint_reference(path):
    problems = []
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            code = strip_strings(line)
            for pat in FORBIDDEN_IN_REFERENCES:
                if re.search(pat, code):
                    problems.append("line {}: non-canonical syntax ({})".format(n, line.strip()))
                    break
    return problems


def check_reference(forge, task, edition):
    """Diagnostics of `forge check` on a task's Forge reference."""
    cmd = [forge, "--edition", edition, "check", "--format", "json", task.reference("forge")]
    try:
        p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
    except subprocess.TimeoutExpired:
        return [{"code": "timeout", "message": "forge check timed out"}]
    out = p.stdout.decode("utf-8", "replace") + p.stderr.decode("utf-8", "replace")
    diags = parse_forge_diagnostics(out)
    if p.returncode != 0 and not diags:
        diags = [{"code": "exit {}".format(p.returncode), "message": out.strip()[-300:]}]
    return diags


def cmd_validate(args):
    forge = find_forge(args.forge)
    tasks = load_tasks(task_selector(args))
    if not tasks:
        sys.exit("no tasks selected")
    problems = []
    ids = {}
    for t in tasks:
        for p in t.problems():
            problems.append("{}: {}".format(t.id or t.dir, p))
        if t.id in ids:
            problems.append("{}: duplicate id (also {})".format(t.id, ids[t.id]))
        ids[t.id] = t.dir
        if os.path.isfile(t.reference("forge")):
            for p in lint_reference(t.reference("forge")):
                problems.append("{}: reference.fg {}".format(t.id, p))
        # tomllib and the fallback parser must agree (CI may run 3.9).
        try:
            mini = load_toml(os.path.join(t.dir, "task.toml"), force_mini=True)
            if mini != t.data:
                problems.append("{}: fallback TOML parser disagrees with tomllib".format(t.id))
        except ValueError as e:
            problems.append("{}: fallback TOML parser: {}".format(t.id, e))

    if args.quick:
        configs = [RunConfig("forge", forge, "vm"), RunConfig("forge", forge, "interp", "2027")]
    else:
        configs = [
            RunConfig("forge", forge, "vm"),
            RunConfig("forge", forge, "interp"),
            RunConfig("forge", forge, "vm", "2027"),
            RunConfig("forge", forge, "interp", "2027"),
        ]
    if not args.no_python:
        configs.append(RunConfig("python", python=args.python))

    jobs = [(t, c) for t in tasks for c in configs]
    failures = []
    start = time.time()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        # References must also be clean for the type checker (warnings
        # included): edition 2027 turns declared-type mismatches into errors.
        checks = {pool.submit(check_reference, forge, t, "2027"): t
                  for t in tasks if os.path.isfile(t.reference("forge"))}
        for fut in concurrent.futures.as_completed(checks):
            for d in fut.result():
                problems.append("{}: reference.fg:{}: {} {} ({})".format(
                    checks[fut].id, d.get("line"), d.get("code"), d.get("message"),
                    d.get("severity")))
        futs = {pool.submit(evaluate, c, t, t.reference(c.lang)): (t, c) for t, c in jobs}
        for fut in concurrent.futures.as_completed(futs):
            t, c = futs[fut]
            r = fut.result()
            if r["status"] != "pass":
                failures.append((t, c, r))
    for t, c, r in sorted(failures, key=lambda x: (x[0].id, x[1].label())):
        problems.append("{} [{}]: {} {} {}".format(
            t.id, c.label(), r["class"], r.get("code") or "", (r.get("message") or "").strip()))
        for d in r.get("diff", []):
            problems.append("    " + d)
        if args.verbose and r.get("stderr"):
            problems.append("    stderr: " + r["stderr"].strip().replace("\n", "\n            "))
    elapsed = time.time() - start
    per_cat = {}
    for t in tasks:
        per_cat[t.category] = per_cat.get(t.category, 0) + 1
    print("forge: {}".format(forge))
    print("tasks: {} ({})".format(
        len(tasks), ", ".join("{} {}".format(k, v) for k, v in sorted(per_cat.items()))))
    print("runs: {} across {} in {:.1f}s".format(
        len(jobs), ", ".join(c.label() for c in configs), elapsed))
    if problems:
        print("\nFAILED ({} problems):".format(len(problems)))
        for p in problems:
            print("  " + p)
        return 1
    print("OK: every reference passes")
    return 0


# ---------------------------------------------------------------------------
# Prompts and providers
# ---------------------------------------------------------------------------


def read_llms():
    with open(LLMS_TXT, encoding="utf-8") as f:
        return f.read()


def system_prompt(lang, context):
    info = LANGS[lang]
    if lang == "forge":
        text = (
            "You write programs in Forge, a programming language whose source "
            "files use the .fg extension and run with `forge run main.fg`. "
        )
    else:
        text = "You write programs in Python 3 using only the standard library. "
    text += (
        "Write a complete, runnable {name} program that solves the user's task. "
        "It must print exactly the requested output to standard output and nothing else. "
        "Reply with exactly one fenced code block (```{fence}) containing the whole "
        "program, and no other text."
    ).format(name=info["name"], fence=info["fence"])
    if lang == "forge" and context in ("llms", "llms+errors"):
        text += "\n\nForge language reference (llms.txt):\n\n" + read_llms()
    return text


def user_prompt(task):
    return task.prompt


def extract_code(response, lang):
    """The code block to run: the first block fenced with the language's
    tag, else the longest fenced block, else the raw text."""
    blocks = re.findall(r"```([\w+-]*)[^\n]*\n(.*?)(?:```|\Z)", response, re.S)
    tags = {"forge": ("forge", "fg"), "python": ("python", "py", "python3")}[lang]
    for tag, body in blocks:
        if tag.lower() in tags and body.strip():
            return body, "fenced"
    blocks = [b for _, b in blocks if b.strip()]
    if blocks:
        return max(blocks, key=len), "fenced-other"
    if response.strip():
        return response.strip() + "\n", "raw"
    return "", "none"


class AnthropicProvider:
    def __init__(self, model, max_tokens, temperature):
        self.key = os.environ.get("ANTHROPIC_API_KEY")
        if not self.key:
            sys.exit("ANTHROPIC_API_KEY is not set")
        base = os.environ.get("ANTHROPIC_BASE_URL", "https://api.anthropic.com").rstrip("/")
        self.url = base + "/v1/messages"
        self.model = model
        self.max_tokens = max_tokens
        self.temperature = temperature

    def complete(self, system, messages, task):
        body = {
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": messages,
        }
        if self.temperature is not None:
            body["temperature"] = self.temperature
        data = json.dumps(body).encode("utf-8")
        delay = 2.0
        for attempt in range(6):
            req = urllib.request.Request(self.url, data=data, method="POST", headers={
                "content-type": "application/json",
                "x-api-key": self.key,
                "anthropic-version": "2023-06-01",
            })
            try:
                with urllib.request.urlopen(req, timeout=300) as resp:
                    payload = json.loads(resp.read().decode("utf-8"))
                return "".join(
                    b.get("text", "") for b in payload.get("content", []) if b.get("type") == "text")
            except urllib.error.HTTPError as e:
                if e.code in (429, 500, 502, 503, 504, 529) and attempt < 5:
                    time.sleep(delay + random.random())
                    delay *= 2
                    continue
                raise RuntimeError("API error {}: {}".format(
                    e.code, e.read().decode("utf-8", "replace")[:500]))
            except (urllib.error.URLError, socket.timeout) as e:
                if attempt < 5:
                    time.sleep(delay)
                    delay *= 2
                    continue
                raise RuntimeError("API unreachable: {}".format(e))


class CommandProvider:
    """Pipes the conversation (system prompt, then the messages) into a
    shell command and reads the reply from its stdout."""

    def __init__(self, command, timeout):
        self.command = command
        self.timeout = timeout

    def complete(self, system, messages, task):
        parts = [system]
        for m in messages:
            label = "USER" if m["role"] == "user" else "ASSISTANT"
            parts.append("### {}\n\n{}".format(label, m["content"]))
        env = dict(os.environ, FORGE_EVAL_TASK_ID=task.id)
        p = subprocess.run(
            self.command, shell=True, input="\n\n".join(parts).encode("utf-8"),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=self.timeout, env=env)
        if p.returncode != 0:
            raise RuntimeError("command failed ({}): {}".format(
                p.returncode, p.stderr.decode("utf-8", "replace")[-500:]))
        return p.stdout.decode("utf-8", "replace")


def find_dir_solution(src, task, lang, answers):
    """Locate a pre-generated solution for `task` under `src`."""
    ext = LANGS[lang]["ext"]
    if task.id in answers:
        return answers[task.id]
    if os.path.isdir(src):
        for rel in (
            "{id}.{ext}",
            "{id}/solution.{ext}",
            "{cat}/{id}/solution.{ext}",
            "{cat}/{id}/reference.{ext}",
        ):
            path = os.path.join(src, rel.format(id=task.id, cat=task.category, ext=ext))
            if os.path.isfile(path):
                with open(path, encoding="utf-8") as f:
                    return f.read()
    return None


def parse_answer_markdown(text, task_ids):
    """`## <task id>` (any heading level, id optionally in backticks)
    followed by a fenced code block; the first block after each heading."""
    answers = {}
    current = None
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r"^#{1,6}\s+(.*)$", line)
        if m:
            current = None
            for tok in re.findall(r"[A-Za-z0-9_]+", m.group(1)):
                if tok in task_ids:
                    current = tok
                    break
        elif line.lstrip().startswith("```") and current and current not in answers:
            body = []
            i += 1
            while i < len(lines) and not lines[i].lstrip().startswith("```"):
                body.append(lines[i])
                i += 1
            answers[current] = "\n".join(body) + "\n"
        elif line.lstrip().startswith("```"):
            i += 1
            while i < len(lines) and not lines[i].lstrip().startswith("```"):
                i += 1
        i += 1
    return answers


def load_answers(src, tasks):
    ids = {t.id for t in tasks}
    paths = []
    if os.path.isfile(src):
        paths = [src]
    elif os.path.isdir(src):
        paths = [os.path.join(src, n) for n in sorted(os.listdir(src)) if n.endswith(".md")]
    answers = {}
    for p in paths:
        with open(p, encoding="utf-8") as f:
            for k, v in parse_answer_markdown(f.read(), ids).items():
                answers.setdefault(k, v)
    return answers


def repair_message(lang, result, evaluation):
    """The feedback for the one repair round of `llms+errors`."""
    if evaluation["class"] == "timeout":
        detail = "The program did not finish within the time limit."
    elif evaluation["class"] == "wrong_output":
        out = "\n".join(evaluation.get("stdout", "").splitlines()[:40])
        detail = ("The program ran, but its output does not match the task. "
                  "Its output was:\n```\n{}\n```".format(out))
    elif lang == "forge":
        diags = parse_forge_diagnostics(evaluation.get("stderr", ""))
        text = "\n".join(json.dumps(d) for d in diags) or evaluation.get("stderr", "")
        detail = ("Running it with `forge --error-format json run main.fg` reported:\n"
                  "```json\n{}\n```".format(text.strip()))
    else:
        tail = "\n".join(evaluation.get("stderr", "").strip().splitlines()[-25:])
        detail = "Running it with `python3 main.py` failed:\n```\n{}\n```".format(tail)
    return (detail + "\n\nFix the program. Reply with the complete corrected program "
            "in exactly one fenced code block.")


def cmd_generate(args):
    lang = args.lang
    tasks = load_tasks(task_selector(args))
    if args.limit:
        tasks = tasks[:args.limit]
    if not tasks:
        sys.exit("no tasks selected")
    out = args.out
    os.makedirs(out, exist_ok=True)
    ext = LANGS[lang]["ext"]
    system = system_prompt(lang, args.context)
    provider = None
    answers = {}
    if args.provider == "anthropic":
        provider = AnthropicProvider(args.model, args.max_tokens, args.temperature)
    elif args.provider == "command":
        if not args.command:
            sys.exit("--provider command needs --command")
        provider = CommandProvider(args.command, args.provider_timeout)
    else:
        if not args.source:
            sys.exit("--provider dir needs --from PATH")
        answers = load_answers(args.source, tasks)
    if args.context == "llms+errors" and args.provider == "dir":
        sys.exit("--context llms+errors needs a provider that can answer (anthropic or command)")
    forge = find_forge(args.forge) if (args.context == "llms+errors" and lang == "forge") else None
    cfg = RunConfig(lang, forge, args.engine, args.edition, args.python)

    meta = {
        "lang": lang,
        "provider": args.provider,
        "model": args.model if args.provider == "anthropic" else (
            args.command if args.provider == "command" else args.source),
        "context": args.context,
        "created": datetime.datetime.now().isoformat(timespec="seconds"),
        "tasks": [t.id for t in tasks],
    }
    if args.context == "llms+errors":
        meta["repair_run"] = {"engine": args.engine, "edition": args.edition}
    with open(os.path.join(out, "meta.json"), "w") as f:
        json.dump(meta, f, indent=2)

    def one(task):
        tdir = os.path.join(out, task.id)
        os.makedirs(tdir, exist_ok=True)
        sol = os.path.join(tdir, "solution." + ext)
        if args.provider == "dir":
            code = find_dir_solution(args.source, task, lang, answers)
            if code is None:
                if os.path.exists(sol):
                    os.remove(sol)
                return task.id, "missing"
            with open(sol, "w", encoding="utf-8") as f:
                f.write(code)
            return task.id, "ok"
        if args.skip_existing and os.path.isfile(sol):
            return task.id, "kept"
        messages = [{"role": "user", "content": user_prompt(task)}]
        with open(os.path.join(tdir, "prompt.txt"), "w", encoding="utf-8") as f:
            f.write(messages[0]["content"])
        try:
            reply = provider.complete(system, messages, task)
        except Exception as e:  # recorded as no_code
            with open(os.path.join(tdir, "error.txt"), "w") as f:
                f.write(str(e))
            return task.id, "error: {}".format(e)
        with open(os.path.join(tdir, "response.txt"), "w", encoding="utf-8") as f:
            f.write(reply)
        code, how = extract_code(reply, lang)
        with open(sol, "w", encoding="utf-8") as f:
            f.write(code)
        status = "ok ({})".format(how)
        if args.context == "llms+errors":
            ev = evaluate(cfg, task, sol)
            if ev["status"] != "pass" and ev["class"] != "no_code":
                feedback = repair_message(lang, None, ev)
                messages += [{"role": "assistant", "content": reply},
                             {"role": "user", "content": feedback}]
                with open(os.path.join(tdir, "feedback.txt"), "w", encoding="utf-8") as f:
                    f.write(feedback)
                try:
                    reply2 = provider.complete(system, messages, task)
                except Exception as e:
                    return task.id, status + "; repair error: {}".format(e)
                with open(os.path.join(tdir, "response2.txt"), "w", encoding="utf-8") as f:
                    f.write(reply2)
                code2, _ = extract_code(reply2, lang)
                with open(os.path.join(tdir, "solution2." + ext), "w", encoding="utf-8") as f:
                    f.write(code2)
                status += "; repaired"
        return task.id, status

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for tid, status in pool.map(one, tasks):
            if args.verbose or not status.startswith("ok"):
                print("{}: {}".format(tid, status))
    print("wrote {} ({} tasks, {} {})".format(out, len(tasks), LANGS[lang]["name"], args.context))
    return 0


# ---------------------------------------------------------------------------
# score
# ---------------------------------------------------------------------------


def pct(n, d):
    return round(100.0 * n / d, 1) if d else 0.0


def cmd_score(args):
    run = args.run
    with open(os.path.join(run, "meta.json")) as f:
        meta = json.load(f)
    lang = meta["lang"]
    ext = LANGS[lang]["ext"]
    all_tasks = {t.id: t for t in load_tasks(task_selector(args))}
    tasks = [all_tasks[i] for i in meta.get("tasks", []) if i in all_tasks]
    forge = find_forge(args.forge) if lang == "forge" else None
    cfg = RunConfig(lang, forge, args.engine, args.edition, args.python,
                    grant_stdin_file=args.grant_stdin_file)

    def one(task):
        tdir = os.path.join(run, task.id)
        r = evaluate(cfg, task, os.path.join(tdir, "solution." + ext))
        r["id"] = task.id
        r["category"] = task.category
        r2 = os.path.join(tdir, "solution2." + ext)
        if os.path.isfile(r2):
            rr = evaluate(cfg, task, r2)
            r["repair"] = {k: rr.get(k) for k in ("status", "class", "code", "message")}
        return r

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(one, tasks))

    report = build_report(meta, cfg, results)
    name = args.report_name
    with open(os.path.join(run, name + ".json"), "w") as f:
        json.dump(report, f, indent=2)
    with open(os.path.join(run, name + ".md"), "w") as f:
        f.write(render_report_md(report))
    s = report["summary"]
    print("{}: pass@1 {}/{} = {}%{}".format(
        run, s["passed"], s["total"], s["pass_at_1"],
        "  pass@2 {}%".format(s["pass_at_2"]) if "pass_at_2" in s else ""))
    print("wrote {}".format(os.path.join(run, name + ".md")))
    return 0


def build_report(meta, cfg, results):
    total = len(results)
    passed = sum(1 for r in results if r["status"] == "pass")
    summary = {"total": total, "passed": passed, "pass_at_1": pct(passed, total)}
    has_repair = meta.get("context") == "llms+errors"
    if has_repair:
        p2 = sum(1 for r in results if r["status"] == "pass"
                 or r.get("repair", {}).get("status") == "pass")
        summary["passed_at_2"] = p2
        summary["pass_at_2"] = pct(p2, total)
    by_cat = {}
    for r in results:
        c = by_cat.setdefault(r["category"], {"total": 0, "passed": 0})
        c["total"] += 1
        c["passed"] += r["status"] == "pass"
        if has_repair:
            c["passed_at_2"] = c.get("passed_at_2", 0) + (
                r["status"] == "pass" or r.get("repair", {}).get("status") == "pass")
    for c in by_cat.values():
        c["pass_at_1"] = pct(c["passed"], c["total"])
        if has_repair:
            c["pass_at_2"] = pct(c["passed_at_2"], c["total"])
    failures = {k: 0 for k in FAILURE_CLASSES}
    causes = {}
    for r in results:
        if r["status"] == "pass":
            continue
        failures[r["class"]] = failures.get(r["class"], 0) + 1
        key = (r["class"], r.get("code") or "", message_prefix(r.get("message")))
        entry = causes.setdefault(key, {"class": key[0], "code": key[1],
                                        "message": key[2], "count": 0, "tasks": []})
        entry["count"] += 1
        entry["tasks"].append(r["id"])
    top = sorted(causes.values(), key=lambda e: (-e["count"], e["class"], e["code"], e["message"]))
    by_code = {}
    for r in results:
        if r["status"] != "pass" and r.get("code"):
            by_code[r["code"]] = by_code.get(r["code"], 0) + 1
    return {
        "meta": meta,
        "settings": {
            "lang": cfg.lang,
            "engine": cfg.engine if cfg.lang == "forge" else None,
            "edition": (cfg.edition or "2026") if cfg.lang == "forge" else None,
            "scored": datetime.datetime.now().isoformat(timespec="seconds"),
        },
        "summary": summary,
        "by_category": dict(sorted(by_cat.items())),
        "failures": failures,
        "by_code": dict(sorted(by_code.items(), key=lambda kv: -kv[1])),
        "top_causes": top,
        "tasks": sorted(results, key=lambda r: (r["category"], r["id"])),
    }


def md_escape(text):
    return (text or "").replace("|", "\\|").replace("\n", " ")


def render_report_md(report):
    meta, st, s = report["meta"], report["settings"], report["summary"]
    lines = ["# Forge eval report", ""]
    lines.append("- Language: **{}**".format(LANGS[st["lang"]]["name"]))
    if st["lang"] == "forge":
        lines.append("- Engine: {}, edition {}".format(st["engine"], st["edition"]))
    lines.append("- Provider: {} ({})".format(meta.get("provider"), meta.get("model")))
    lines.append("- Context: {}".format(meta.get("context")))
    lines.append("- pass@1: **{}%** ({}/{})".format(s["pass_at_1"], s["passed"], s["total"]))
    if "pass_at_2" in s:
        lines.append("- pass@2 (after one repair round): **{}%** ({}/{})".format(
            s["pass_at_2"], s["passed_at_2"], s["total"]))
    lines += ["", "## By category", "",
              "| Category | Tasks | Passed | pass@1 |" + (" pass@2 |" if "pass_at_2" in s else ""),
              "|---|---:|---:|---:|" + ("---:|" if "pass_at_2" in s else "")]
    for cat, c in report["by_category"].items():
        row = "| {} | {} | {} | {}% |".format(cat, c["total"], c["passed"], c["pass_at_1"])
        if "pass_at_2" in s:
            row += " {}% |".format(c["pass_at_2"])
        lines.append(row)
    lines += ["", "## Failures by class", "", "| Class | Count |", "|---|---:|"]
    for k, v in report["failures"].items():
        lines.append("| {} | {} |".format(k, v))
    lines += ["", "## Top failure causes", ""]
    if report["top_causes"]:
        lines += ["| Count | Class | Code | Message | Tasks |", "|---:|---|---|---|---|"]
        for e in report["top_causes"][:25]:
            lines.append("| {} | {} | {} | {} | {} |".format(
                e["count"], e["class"], e["code"] or "", md_escape(e["message"]),
                ", ".join(e["tasks"][:6]) + (" ..." if len(e["tasks"]) > 6 else "")))
    else:
        lines.append("None: every task passed.")
    failed = [r for r in report["tasks"] if r["status"] != "pass"]
    if failed:
        lines += ["", "## Failed tasks", ""]
        for r in failed:
            head = "- `{}` ({}): {}".format(r["id"], r["category"], r["class"])
            if r.get("code"):
                head += " {}".format(r["code"])
            if r.get("message"):
                head += " — {}".format(md_escape(r["message"])[:160])
            if r.get("warnings"):
                head += " (warnings: {})".format(", ".join(r["warnings"]))
            if r.get("repair"):
                head += " → repair: {}".format(r["repair"]["status"])
            lines.append(head)
            for d in r.get("diff", [])[:3]:
                lines.append("  - {}".format(md_escape(d)[:200]))
    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# compare
# ---------------------------------------------------------------------------


def load_report(path, name):
    p = path if path.endswith(".json") else os.path.join(path, name + ".json")
    with open(p) as f:
        return json.load(f)


def run_label(path, report):
    st = report["settings"]
    lab = os.path.basename(os.path.normpath(path if not path.endswith(".json")
                                            else os.path.dirname(path)))
    if st["lang"] == "forge":
        return "{} ({} {}, {})".format(lab, st["engine"], st["edition"], report["meta"]["context"])
    return "{} (python)".format(lab)


def cmd_compare(args):
    reports = [(p, load_report(p, args.report_name)) for p in args.runs]
    labels = [run_label(p, r) for p, r in reports]
    cats = sorted({c for _, r in reports for c in r["by_category"]})
    lines = ["| | " + " | ".join(labels) + " |", "|---|" + "---:|" * len(labels)]
    row = ["**overall pass@1**"]
    for _, r in reports:
        s = r["summary"]
        row.append("**{}%** ({}/{})".format(s["pass_at_1"], s["passed"], s["total"]))
    lines.append("| " + " | ".join(row) + " |")
    if any("pass_at_2" in r["summary"] for _, r in reports):
        row = ["pass@2"]
        for _, r in reports:
            row.append("{}%".format(r["summary"]["pass_at_2"]) if "pass_at_2" in r["summary"] else "")
        lines.append("| " + " | ".join(row) + " |")
    for c in cats:
        row = [c]
        for _, r in reports:
            v = r["by_category"].get(c)
            row.append("{}% ({}/{})".format(v["pass_at_1"], v["passed"], v["total"]) if v else "")
        lines.append("| " + " | ".join(row) + " |")
    for k in FAILURE_CLASSES:
        row = ["fail: " + k]
        for _, r in reports:
            row.append(str(r["failures"].get(k, 0)))
        lines.append("| " + " | ".join(row) + " |")
    if len(reports) == 2:
        a = {t["id"]: t for t in reports[0][1]["tasks"]}
        b = {t["id"]: t for t in reports[1][1]["tasks"]}
        only_a = sorted(i for i in a if i in b and a[i]["status"] == "pass" and b[i]["status"] != "pass")
        only_b = sorted(i for i in a if i in b and b[i]["status"] == "pass" and a[i]["status"] != "pass")
        lines += ["", "Passed only in {}: {}".format(labels[0], ", ".join(only_a) or "none"),
                  "", "Passed only in {}: {}".format(labels[1], ", ".join(only_b) or "none")]
    text = "\n".join(lines) + "\n"
    print(text)
    if args.out:
        with open(args.out, "w") as f:
            f.write(text)
    return 0


# ---------------------------------------------------------------------------
# blind-pack
# ---------------------------------------------------------------------------


def pack_instructions(lang, context, pack_name, answers_path):
    info = LANGS[lang]
    text = [
        "# {} eval pack {}".format(info["name"], pack_name),
        "",
        "You are acting as the model under evaluation. For every task below, write a "
        "complete, runnable **{}** program that solves it on the first try.".format(info["name"]),
        "",
        "Rules:",
        "",
        "- Do **not** run, compile or test any code, and do not use any tool other than "
        "reading this file and writing your answer file.",
    ]
    if lang == "forge" and context == "llms":
        text.append("- The only documentation you may read is `llms.txt` at the repository "
                    "root. Do not open any other file in the repository (in particular "
                    "nothing under `eval/`, `tests/`, `examples/` or `src/`).")
    elif lang == "forge":
        text.append("- Forge is a programming language (source files `.fg`, run with "
                    "`forge run main.fg`). Write what you believe is correct Forge. Do not "
                    "read any documentation or any other file in the repository.")
    else:
        text.append("- Use Python 3 and only the standard library. Do not read any other "
                    "file in the repository.")
    text += [
        "- Each program must print exactly the requested output to standard output and "
        "nothing else.",
        "- Answer format: for each task, a heading `## <task id>` followed by exactly one "
        "fenced code block (```{}) with the whole program. Nothing else is needed.".format(
            info["fence"]),
        "- Write all answers to `{}`.".format(answers_path),
        "",
    ]
    return "\n".join(text)


def cmd_blind_pack(args):
    tasks = load_tasks(task_selector(args))
    if args.limit:
        tasks = tasks[:args.limit]
    if args.shuffle is not None:
        random.Random(args.shuffle).shuffle(tasks)
    os.makedirs(args.out, exist_ok=True)
    size = max(1, args.per_pack)
    packs = [tasks[i:i + size] for i in range(0, len(tasks), size)]
    for n, pack in enumerate(packs, 1):
        answers = os.path.join(args.answers_dir, "{}.md".format(n))
        lines = [pack_instructions(args.lang, args.context, "{} of {}".format(n, len(packs)),
                                   answers), "---", ""]
        for t in pack:
            lines += ["## {}".format(t.id), "", t.prompt, ""]
        path = os.path.join(args.out, "{}.md".format(n))
        with open(path, "w", encoding="utf-8") as f:
            f.write("\n".join(lines))
    print("wrote {} packs ({} tasks) to {}".format(len(packs), len(tasks), args.out))
    print("collect answers with: python3 eval/forge_eval.py generate --provider dir "
          "--from {} --lang {} --context {} --out eval/runs/<name>".format(
              args.answers_dir, args.lang, args.context))
    return 0


# ---------------------------------------------------------------------------
# selftest
# ---------------------------------------------------------------------------

# (task id, Forge program, expected class, expected code)
SELFTEST_CASES = [
    ("temperature_table", "for c in 0..6 {\n    println(c)\n}\n", "syntax_error", "E0002"),
    ("primes_below", "println(primes_up_to(10))\n", "runtime_error", "E0003"),
    ("greet_people", "let xs = [1]\nprintln(xs[3])\n", "runtime_error", "E0009"),
    ("digital_root", "let mut n = 0\nwhile true {\n    n += 1\n}\n", "timeout", None),
    ("fizzbuzz_bazz", "println(1)\n", "wrong_output", None),
    ("leap_years", "", "no_code", None),
]


def cmd_selftest(args):
    """Check the harness itself: scoring and failure classification on
    known-bad programs, answer parsing, code extraction and TOML parsing."""
    forge = find_forge(args.forge)
    failures = []

    def expect(cond, what):
        if not cond:
            failures.append(what)

    tasks = {t.id: t for t in load_tasks(lambda t: t.category == "basics")}
    cfg = RunConfig("forge", forge, args.engine)
    work = tempfile.mkdtemp(prefix="forge-eval-selftest-")
    try:
        jobs = []
        for tid, program, klass, code in SELFTEST_CASES:
            task = tasks[tid]
            if klass == "timeout":
                task.timeout = 2.0
            path = os.path.join(work, tid + ".fg")
            with open(path, "w") as f:
                f.write(program)
            jobs.append((task, path, klass, code))
        ref = tasks["collatz_longest"]
        jobs.append((ref, ref.reference("forge"), "pass", None))
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            results = list(pool.map(lambda j: evaluate(cfg, j[0], j[1]), jobs))
        for (task, _, klass, code), r in zip(jobs, results):
            expect(r["class"] == klass and r.get("code") == code,
                   "{}: expected {} {}, got {} {} ({})".format(
                       task.id, klass, code, r["class"], r.get("code"), r.get("message")))
    finally:
        shutil.rmtree(work, ignore_errors=True)

    md = ("# x\n\n## `leap_years`\n\ntext\n\n```forge\nprintln(1)\n```\n\n"
          "### Task digital_root\n```\nb\n```\n")
    got = parse_answer_markdown(md, {"leap_years", "digital_root"})
    expect(got == {"leap_years": "println(1)\n", "digital_root": "b\n"},
           "parse_answer_markdown: {!r}".format(got))
    code, how = extract_code("Sure!\n```python\nx\n```\n```forge\nprintln(2)\n```", "forge")
    expect((code, how) == ("println(2)\n", "fenced"), "extract_code: {!r}".format((code, how)))
    expect(extract_code("", "forge") == ("", "none"), "extract_code on an empty reply")
    toml = 'a = "x\\ty"\nb = [1, 2.5, true]\nc = """\nline\n"""\n[[request]]\nm = { k = \'v\' }\n'
    parsed = MiniToml(toml).parse()
    expect(parsed == {"a": "x\ty", "b": [1, 2.5, True], "c": "line\n",
                      "request": [{"m": {"k": "v"}}]}, "MiniToml: {!r}".format(parsed))
    expect(message_prefix("undefined variable: 'foo' at 12") == "undefined variable: '_' at N",
           "message_prefix")

    if failures:
        print("selftest FAILED:")
        for f in failures:
            print("  " + f)
        return 1
    print("selftest OK ({} scoring cases)".format(len(SELFTEST_CASES) + 1))
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(prog="forge_eval.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    jobs = max(2, min(16, (os.cpu_count() or 2) * 2))

    def common(p, run_opts=True):
        p.add_argument("--tasks", help="comma-separated task ids")
        p.add_argument("--category", help="comma-separated categories")
        p.add_argument("--jobs", type=int, default=jobs, help="parallel workers")
        if run_opts:
            p.add_argument("--forge", help="forge binary (default: $FORGE_BIN, target/, PATH)")
            p.add_argument("--python", default=sys.executable or "python3",
                           help="python interpreter for Python solutions")
        p.add_argument("-v", "--verbose", action="store_true")

    p = sub.add_parser("validate", help="run every reference solution")
    common(p)
    p.add_argument("--quick", action="store_true",
                   help="Forge on vm (2026) and interp (2027) only, instead of all four")
    p.add_argument("--no-python", action="store_true", help="skip the Python references")
    p.set_defaults(func=cmd_validate)

    p = sub.add_parser("selftest", help="check the harness's own scoring and parsing")
    common(p)
    p.add_argument("--engine", choices=["vm", "interp"], default="vm")
    p.set_defaults(func=cmd_selftest)

    p = sub.add_parser("generate", help="produce solutions with a provider")
    common(p)
    p.add_argument("--provider", choices=["anthropic", "command", "dir"], required=True)
    p.add_argument("--lang", choices=sorted(LANGS), required=True)
    p.add_argument("--context", choices=CONTEXTS, default="llms")
    p.add_argument("--model", default=DEFAULT_MODEL)
    p.add_argument("--max-tokens", type=int, default=8192)
    p.add_argument("--temperature", type=float, default=None)
    p.add_argument("--command", help="shell command for --provider command (prompt on stdin)")
    p.add_argument("--provider-timeout", type=float, default=600)
    p.add_argument("--from", dest="source",
                   help="--provider dir: a directory of solutions or Markdown answer files")
    p.add_argument("--out", required=True, help="run directory, e.g. eval/runs/<name>")
    p.add_argument("--limit", type=int)
    p.add_argument("--skip-existing", action="store_true")
    p.add_argument("--engine", choices=["vm", "interp"], default="vm",
                   help="engine for the llms+errors repair run")
    p.add_argument("--edition", help="edition for the llms+errors repair run")
    p.set_defaults(func=cmd_generate)

    p = sub.add_parser("score", help="run and score a run directory")
    common(p)
    p.add_argument("run")
    p.add_argument("--engine", choices=["vm", "interp"], default="vm")
    p.add_argument("--edition", help="e.g. 2027 (default: the forge default edition)")
    p.add_argument("--report-name", default="report")
    p.add_argument("--grant-stdin-file", action="store_true",
                   help="counterfactual: also allow reading /dev/stdin as a file")
    p.set_defaults(func=cmd_score)

    p = sub.add_parser("compare", help="compare scored runs")
    p.add_argument("runs", nargs="+")
    p.add_argument("--report-name", default="report")
    p.add_argument("--out", help="also write the table to this file")
    p.set_defaults(func=cmd_compare)

    p = sub.add_parser("blind-pack", help="write task packs for agents acting as models")
    common(p, run_opts=False)
    p.add_argument("--lang", choices=sorted(LANGS), default="forge")
    p.add_argument("--context", choices=["none", "llms"], default="none")
    p.add_argument("--per-pack", type=int, default=10)
    p.add_argument("--limit", type=int)
    p.add_argument("--shuffle", type=int, metavar="SEED")
    p.add_argument("--out", default=os.path.join(EVAL_DIR, "packs"))
    p.add_argument("--answers-dir", default=os.path.join("eval", "answers"))
    p.set_defaults(func=cmd_blind_pack)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
