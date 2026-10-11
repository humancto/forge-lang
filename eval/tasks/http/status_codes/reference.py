import sys, urllib.request, urllib.error
lines = sys.stdin.read().splitlines()
base = lines[0].strip()
ok = bad = 0
for path in lines[1:]:
    path = path.strip()
    if not path:
        continue
    try:
        with urllib.request.urlopen(base + path) as r:
            status = r.status
    except urllib.error.HTTPError as e:
        status = e.code
    cls = "ok" if 200 <= status < 300 else ("client error" if status < 500 else "server error")
    if cls == "ok":
        ok += 1
    else:
        bad += 1
    print(f"{path}: {status} {cls}")
print(f"ok: {ok}, failed: {bad}")
