import re, sys
text = sys.stdin.read()
head, template = text.split("---\n", 1)
values = dict(l.split("=", 1) for l in head.splitlines() if "=" in l)
missing = []
def repl(m):
    k = m.group(1)
    if k in values:
        return values[k]
    if k not in missing:
        missing.append(k)
    return m.group(0)
sys.stdout.write(re.sub(r"\{\{(\w+)\}\}", repl, template))
print("missing: " + (",".join(missing) or "none"))
