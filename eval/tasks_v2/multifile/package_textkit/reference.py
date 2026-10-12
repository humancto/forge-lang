import re
import sys


def slug(s):
    return "-".join(re.findall(r"[a-z0-9]+", s.lower()))


def wrap(text, width):
    lines, cur = [], ""
    for w in text.split():
        if not cur:
            cur = w
        elif len(cur) + 1 + len(w) <= width:
            cur += " " + w
        else:
            lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return lines


sections = []
para = []
for line in sys.stdin.read().splitlines():
    if line.startswith("# "):
        sections.append({"title": line[2:].strip(), "paras": []})
        para = None
    elif line.strip() == "":
        para = None
    else:
        if para is None:
            para = []
            sections[-1]["paras"].append(para)
        para.append(line.strip())

used = set()
print("Contents")
for n, s in enumerate(sections, 1):
    base = slug(s["title"])
    anchor, k = base, 1
    while anchor in used:
        k += 1
        anchor = "{}-{}".format(base, k)
    used.add(anchor)
    print("{}. {} (#{})".format(n, s["title"], anchor))
for s in sections:
    paras = [" ".join(p) for p in s["paras"]]
    print()
    print("== {} ({} words) ==".format(s["title"], sum(len(p.split()) for p in paras)))
    for i, p in enumerate(paras):
        if i:
            print()
        for l in wrap(p, 40):
            print(l)
