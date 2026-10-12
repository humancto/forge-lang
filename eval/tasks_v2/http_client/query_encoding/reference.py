import json
import sys
import urllib.parse
import urllib.request

lines = [l.strip() for l in sys.stdin.buffer.read().decode("utf-8").splitlines() if l.strip()]
base = lines[0]
params = dict(l.split("=", 1) for l in lines[1:])
with urllib.request.urlopen(base + "/echo?" + urllib.parse.urlencode(params)) as r:
    got = json.loads(r.read())["query"]
for k in sorted(got):
    print("{} -> [{}]".format(k, got[k]))
print("round trip: " + ("ok" if got == params else "mismatch"))
