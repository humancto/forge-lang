import re
import sys

HEAD = re.compile(r"^(?P<date>\d{4}-\d{2}-\d{2}) (?P<time>\d{2}:\d{2}:\d{2}) \[(?P<level>[A-Z]+)\] "
                  r"(?P<module>[a-z]+(?:\.[a-z]+)*)(?P<rest>(?: .*)?)$")
PAIR = re.compile(r'([a-z_]+)=("(?:[^"\\]|\\.)*"|\S+)')


def redact(v):
    v = re.sub(r"([A-Za-z0-9._%+-])[A-Za-z0-9._%+-]*@([A-Za-z0-9.-]+\.[A-Za-z]{2,})", r"\1***@\2", v)
    v = re.sub(r"\b(\d{1,3}\.\d{1,3}\.\d{1,3})\.\d{1,3}\b", r"\1.x", v)
    return v


levels = {}
users = set()
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    if not line.strip():
        continue
    m = HEAD.match(line)
    if not m:
        print("skipped line {}".format(n))
        continue
    fields = []
    for k, v in PAIR.findall(m.group("rest")):
        if v.startswith('"'):
            v = re.sub(r'\\(.)', r"\1", v[1:-1])
        if k == "user":
            users.add(v)
        fields.append("{}={}".format(k, redact(v)))
    level = m.group("level")
    levels[level] = levels.get(level, 0) + 1
    print("{}T{} {:<5} {} {}".format(m.group("date"), m.group("time"), level, m.group("module"),
                                     "; ".join(fields)).rstrip())
print("levels: " + " ".join("{}={}".format(k, levels[k]) for k in sorted(levels)))
print("users: " + ", ".join(sorted(users)))
