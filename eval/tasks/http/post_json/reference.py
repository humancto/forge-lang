import json, sys, urllib.request
base = sys.stdin.readline().strip()
data = json.dumps({"user": "ada", "items": [3, 1, 2], "express": True}).encode()
req = urllib.request.Request(base + "/echo?ref=eval", data=data, method="POST",
                             headers={"Content-Type": "application/json"})
with urllib.request.urlopen(req) as r:
    reply = json.loads(r.read())
print(f"method: {reply['method']}")
print(f"ref: {reply['query']['ref']}")
print(f"items sum: {sum(reply['body']['items'])}")
print(f"express: {str(reply['body']['express']).lower()}")
print("json content type: " + ("yes" if reply["content_type"].startswith("application/json") else "no"))
