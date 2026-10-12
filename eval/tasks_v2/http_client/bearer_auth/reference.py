import json
import sys
import urllib.error
import urllib.request

base, token = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()][:2]


def get(path, headers=None):
    req = urllib.request.Request(base + path, headers=headers or {})
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())


auth = {"Authorization": "Bearer " + token}
st, body = get("/auth/profile.json")
print("no token: {} {}".format(st, body["error"]))
st, body = get("/auth/profile.json", {"Authorization": "Bearer not-the-token"})
print("wrong token: {} {}".format(st, body["error"]))
_, profile = get("/auth/profile.json", auth)
print("profile: {} <{}> ({})".format(profile["name"], profile["email"], profile["plan"]))
_, orders = get("/auth/orders.json", auth)
cents = sum(o["total_cents"] for o in orders["orders"])
print("orders: {} total {}.{:02d}".format(len(orders["orders"]), cents // 100, cents % 100))
_, seen = get("/headers", dict(auth, **{"Accept": "application/json",
                                        "X-Request-Id": "req-{}".format(profile["id"]),
                                        "X-Client": "eval/1.0"}))
for k in sorted(seen):
    print("{}: {}".format(k, seen[k]))
