import re, sys
problems, cfg = [], {}
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    s = line.strip()
    if not s or s.startswith("#"):
        continue
    if "=" not in s:
        problems.append(f"line {n}: expected key = value")
        continue
    k, v = s.split("=", 1)
    cfg[k.strip()] = v.strip()
for k in ["host", "port", "mode"]:
    if k not in cfg:
        problems.append(f"missing key: {k}")
if "port" in cfg and not (re.fullmatch(r"\d+", cfg["port"]) and 1 <= int(cfg["port"]) <= 65535):
    problems.append(f"invalid port: {cfg['port']}")
if "mode" in cfg and cfg["mode"] not in ("dev", "prod"):
    problems.append(f"invalid mode: {cfg['mode']}")
if "workers" in cfg and not (re.fullmatch(r"\d+", cfg["workers"]) and int(cfg["workers"]) > 0):
    problems.append(f"invalid workers: {cfg['workers']}")
if problems:
    print(f"{len(problems)} problems:")
    for p in problems:
        print("- " + p)
else:
    print("config ok")
