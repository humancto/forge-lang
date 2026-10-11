import json, sys
users = json.load(sys.stdin)
act = sorted((u for u in users if u["active"]), key=lambda u: (u["age"], u["name"]))
for u in act:
    print(f"{u['name']} ({u['age']})")
print(f"active: {len(act)} of {len(users)}")
