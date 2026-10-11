import re, sys
pat = re.compile(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} \[(DEBUG|INFO|WARN|ERROR)\] (.*)$")
counts = {k: 0 for k in ["DEBUG", "INFO", "WARN", "ERROR"]}
invalid = 0
errors = []
for line in sys.stdin.read().splitlines():
    m = pat.match(line)
    if not m:
        invalid += 1
        continue
    counts[m.group(1)] += 1
    if m.group(1) == "ERROR":
        errors.append(m.group(2))
for k, v in counts.items():
    print(f"{k}: {v}")
print(f"invalid: {invalid}")
for e in errors:
    print("! " + e)
