import json
cfg = json.load(open("config.json"))
old = cfg["version"]
parts = old.split(".")
parts[2] = str(int(parts[2]) + 1)
cfg["version"] = ".".join(parts)
if "beta" not in cfg["features"]:
    cfg["features"].append("beta")
cfg["updated_by"] = "eval"
cfg.pop("deprecated", None)
with open("config.json", "w") as f:
    f.write(json.dumps(cfg, indent=2, sort_keys=True) + "\n")
print(f"version: {old} -> {cfg['version']}")
print(f"features: {len(cfg['features'])}")
