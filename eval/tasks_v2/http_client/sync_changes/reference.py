import json
import sys
import urllib.request

base = sys.stdin.read().strip()


def call(method, path, body=None):
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(base + path, data=data, method=method, headers=headers)
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read())


def compact(v):
    return json.dumps(v, separators=(",", ":"), ensure_ascii=False)


local = {r["id"]: r for r in call("GET", "/local.json")}
remote = {r["id"]: r for r in call("GET", "/remote.json")}
creates = sorted(i for i in local if i not in remote)
updates = sorted(i for i in local if i in remote and local[i]["version"] > remote[i]["version"])
deletes = sorted(i for i in remote if i not in local)
conflicts = sorted(i for i in local if i in remote and local[i]["version"] < remote[i]["version"])
unchanged = [i for i in local if i in remote and local[i]["version"] == remote[i]["version"]]

responses = []
for i in creates:
    r = local[i]
    responses.append(call("POST", "/echo", {"id": r["id"], "version": r["version"],
                                            "title": r["title"], "tags": r["tags"]}))
for i in updates:
    r = local[i]
    responses.append(call("PUT", "/echo?id=" + i, {"version": r["version"], "title": r["title"],
                                                   "tags": r["tags"]}))
for i in deletes:
    responses.append(call("DELETE", "/echo?id=" + i))
for e in responses:
    print("{} query={} body={}".format(e["method"], compact(e["query"]), compact(e["body"])))
for i in conflicts:
    print("conflict {}: local v{} < remote v{}".format(i, local[i]["version"], remote[i]["version"]))
print("created={} updated={} deleted={} conflicts={} unchanged={}".format(
    len(creates), len(updates), len(deletes), len(conflicts), len(unchanged)))
