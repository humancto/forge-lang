import json, sys
teams = json.load(sys.stdin)
out = [{"team": t, "size": len(m), "leads": [x["name"] for x in m if x["role"] == "lead"]}
       for t, m in sorted(teams.items())]
print(json.dumps(out, separators=(",", ":")))
