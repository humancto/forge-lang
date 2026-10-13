import json
import sys
import urllib.error
import urllib.request

lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
base = lines[0]
ok = errors = 0
for line in lines[1:]:
    label, target = line.split(" ", 1)
    url = target if target.startswith("http://") else base + target
    try:
        with urllib.request.urlopen(url, timeout=5) as r:
            status, body = r.status, r.read()
    except urllib.error.HTTPError as e:
        status, body = e.code, e.read()
    except (urllib.error.URLError, OSError):
        print("{}: connection failed".format(label))
        errors += 1
        continue
    if status >= 400:
        print("{}: HTTP {}".format(label, status))
        errors += 1
        continue
    try:
        doc = json.loads(body)
    except ValueError:
        print("{}: invalid JSON".format(label))
        errors += 1
        continue
    print("{}: ok {}".format(label, json.dumps(doc, separators=(",", ":"))))
    ok += 1
print("ok={} errors={}".format(ok, errors))
