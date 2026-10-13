import sys

# Produces the expected output of the Forge modules driven by files/main.fg.
lines = sys.stdin.read().split("\n")
if lines and lines[-1] == "":
    lines.pop()
NAMES = ["trim", "lower", "upper", "dedupe", "nonempty"]
print("plugins: " + ", ".join("<{}>".format(n) for n in NAMES))


def apply(spec, xs):
    if spec == "trim":
        return [x.strip() for x in xs]
    if spec == "lower":
        return [x.lower() for x in xs]
    if spec == "upper":
        return [x.upper() for x in xs]
    if spec == "dedupe":
        seen, out = set(), []
        for x in xs:
            if x not in seen:
                seen.add(x)
                out.append(x)
        return out
    if spec == "nonempty":
        return [x for x in xs if x != ""]
    return [x for x in xs if spec[5:] in x]


specs = [s for s in lines[0].split(" ") if s]
bad = [s for s in specs if s not in NAMES and not (s.startswith("grep=") and len(s) > 5)]
if bad:
    print("error: unknown plugin " + bad[0])
else:
    out = lines[1:]
    for s in specs:
        out = apply(s, out)
    for x in out:
        print("| " + x)
    print("{} lines".format(len(out)))
