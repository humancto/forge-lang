import json, sys
total = 0
for n, line in enumerate(sys.stdin.read().splitlines(), 1):
    if not line.strip():
        continue
    try:
        obj = json.loads(line)
    except ValueError:
        print(f"{n}: invalid json")
        continue
    if (not isinstance(obj, dict) or not isinstance(obj.get("sku"), str)
            or not isinstance(obj.get("qty"), int) or isinstance(obj.get("qty"), bool)):
        print(f"{n}: bad record")
        continue
    total += obj["qty"]
    print(f"{n}: {obj['sku']} x{obj['qty']}")
print(f"total qty: {total}")
