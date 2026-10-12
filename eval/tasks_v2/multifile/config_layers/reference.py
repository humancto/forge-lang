import json
import sys

DEFAULTS = {
    "server": {"host": "127.0.0.1", "port": 8080, "tls": False},
    "db": {"url": "sqlite://app.db", "pool": 5},
    "log_level": "info",
    "features": [],
}
LEVELS = ["debug", "info", "warn", "error"]


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def validate(cfg):
    errors = []
    port = cfg["server"]["port"]
    if not is_int(port) or not 1 <= port <= 65535:
        errors.append("server.port must be an integer from 1 to 65535")
    if not isinstance(cfg["server"]["tls"], bool):
        errors.append("server.tls must be true or false")
    pool = cfg["db"]["pool"]
    if not is_int(pool) or not 1 <= pool <= 100:
        errors.append("db.pool must be an integer from 1 to 100")
    if cfg["log_level"] not in LEVELS:
        errors.append("log_level must be one of " + ", ".join(LEVELS))
    f = cfg["features"]
    if not isinstance(f, list) or not all(isinstance(x, str) for x in f):
        errors.append("features must be a list of strings")
    return errors


class ObjectError(Exception):
    pass


def merge(defaults, user, prefix):
    out = dict(defaults)
    for k, v in user.items():
        path = prefix + k
        if k not in defaults:
            print("warning: unknown key " + path)
            continue
        if isinstance(defaults[k], dict):
            if not isinstance(v, dict):
                raise ObjectError(path)
            out[k] = merge(defaults[k], v, path + ".")
        else:
            out[k] = v
    return out


try:
    user = json.loads(sys.stdin.read())
except ValueError:
    user = None
if not isinstance(user, dict):
    print("error: invalid JSON")
    sys.exit(2)
try:
    cfg = merge(DEFAULTS, user, "")
except ObjectError as e:
    print("error: {} must be an object".format(e))
    sys.exit(2)
errors = validate(cfg)
if errors:
    for e in errors:
        print("error: " + e)
    sys.exit(1)
print("ok")
print(json.dumps(cfg, separators=(",", ":")))
