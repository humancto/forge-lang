import json
import sys
import urllib.error
import urllib.request

lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
base = lines[0]
ok = failed = requests = 0


def get(url):
    try:
        with urllib.request.urlopen(url) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


for path in lines[1:]:
    attempts = 0
    while True:
        attempts += 1
        requests += 1
        status, body = get(base + path)
        if 200 <= status < 300:
            ok += 1
            print("{}: ok after {} attempt(s), server saw attempt {}".format(
                path, attempts, json.loads(body)["attempt"]))
            break
        if status < 500:
            failed += 1
            print("{}: failed with {} after {} attempt(s)".format(path, status, attempts))
            break
        if attempts == 4:
            failed += 1
            print("{}: gave up after 4 attempts (last status {})".format(path, status))
            break
print("ok={} failed={} requests={}".format(ok, failed, requests))
