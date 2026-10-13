import sys

subs = []
next_token = [1]


def on(event, handler):
    t = next_token[0]
    next_token[0] += 1
    subs.append((t, event, handler))
    return t


def make(kind, arg):
    if kind == "upper":
        return lambda p: p.upper()
    if kind == "len":
        return lambda p: str(len(p))
    if kind == "prefix":
        return lambda p: arg + p
    calls = [0]

    def count(p):
        calls[0] += 1
        return "call #{}".format(calls[0])
    return count


for line in sys.stdin.read().splitlines():
    if not line.strip():
        continue
    parts = line.split(" ")
    cmd = parts[0]
    if cmd == "sub":
        kind = parts[2]
        arg = " ".join(parts[3:]) if kind == "prefix" else None
        print("subscribed {}".format(on(parts[1], make(kind, arg))))
    elif cmd == "pub":
        event = parts[1]
        payload = line.split(" ", 2)[2]
        results = [h(payload) for _, e, h in subs if e == event]
        print("{}: {}".format(event, " | ".join(results) if results else "no listeners"))
    elif cmd == "off":
        t = int(parts[1])
        before = len(subs)
        subs[:] = [s for s in subs if s[0] != t]
        print(("removed {}" if len(subs) < before else "unknown {}").format(t))
    elif cmd == "count":
        n = sum(1 for _, e, _ in subs if e == parts[1])
        print("{} has {} listener(s)".format(parts[1], n))
