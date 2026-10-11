import sys
ages = []
for line in sys.stdin.read().splitlines():
    if not line.strip():
        continue
    d = {}
    for pair in line.split(";"):
        if "=" in pair:
            k, v = pair.split("=", 1)
            d[k] = v
    name = d.get("name") or "anonymous"
    age = d.get("age") or ""
    age = age if age.isdigit() else "?"
    if age != "?":
        ages.append(int(age))
    city = d.get("city") or "unknown"
    print(f"{name} | {age} | {city}")
print("average known age: " + (str(sum(ages) // len(ages)) if ages else "n/a"))
