a = [("bolts", 120), ("nuts", 300), ("washers", 75), ("screws", 40)]
b = [("nuts", 200), ("screws", 60), ("rivets", 15), ("bolts", 5), ("anchors", 0)]
tot = {}
for k, v in a + b:
    tot[k] = tot.get(k, 0) + v
items = sorted(((k, v) for k, v in tot.items() if v > 0), key=lambda kv: (-kv[1], kv[0]))
for k, v in items:
    print(f"{k}: {v}")
print(f"total: {sum(tot.values())}")
