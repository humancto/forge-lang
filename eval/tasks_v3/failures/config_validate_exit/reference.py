import json
import sys

KNOWN = ["name", "port", "workers", "tls", "routes"]


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


try:
    cfg = json.loads(sys.stdin.read())
except ValueError:
    sys.stderr.write("error: invalid JSON\n")
    sys.exit(2)
if not isinstance(cfg, dict):
    sys.stderr.write("error: config must be an object\n")
    sys.exit(2)

errors = []
for k in cfg:
    if k not in KNOWN:
        sys.stderr.write("warning: unknown key {}\n".format(k))
if "name" not in cfg:
    errors.append("name: is required")
elif not isinstance(cfg["name"], str) or cfg["name"] == "":
    errors.append("name: must be a non-empty string")
port = cfg.get("port", 8080)
if not is_int(port) or not 1 <= port <= 65535:
    errors.append("port: must be an integer from 1 to 65535")
workers = cfg.get("workers", 4)
if not is_int(workers) or workers < 1:
    errors.append("workers: must be a positive integer")
tls = cfg.get("tls")
if "tls" in cfg:
    if not isinstance(tls, dict):
        errors.append("tls: must be an object")
    else:
        for f in ("cert", "key"):
            if not isinstance(tls.get(f), str):
                errors.append("tls.{}: is required".format(f))
routes = cfg.get("routes")
seen = set()
if not isinstance(routes, list) or not routes:
    errors.append("routes: must be a non-empty array")
else:
    for i, r in enumerate(routes):
        if not isinstance(r, dict):
            errors.append("routes[{}]: must be an object".format(i))
            continue
        p = r.get("path")
        if not isinstance(p, str) or not p.startswith("/"):
            errors.append("routes[{}].path: must start with /".format(i))
        elif p in seen:
            errors.append("routes[{}].path: duplicate {}".format(i, p))
        else:
            seen.add(p)
        t = r.get("target")
        if not isinstance(t, str) or t == "":
            errors.append("routes[{}].target: is required".format(i))
if errors:
    for e in errors:
        sys.stderr.write("error: {}\n".format(e))
    sys.stderr.write("{} errors\n".format(len(errors)))
    sys.exit(3)
print("name={}".format(cfg["name"]))
print("port={}".format(port))
print("workers={}".format(workers))
print("tls={}".format("on" if "tls" in cfg else "off"))
print("routes={}".format(len(routes)))
for r in sorted(routes, key=lambda r: r["path"]):
    print("{} -> {}".format(r["path"], r["target"]))
