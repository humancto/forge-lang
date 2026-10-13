import sys


def dollars(c):
    return "{}.{:02d}".format(c // 100, c % 100)


def amount(s):
    return int(s) if s.isdigit() and int(s) > 0 else None


bal = {}
frozen = set()
applied = rej_frozen = rej_insuf = invalid = 0
peak = (-1, "", 0)
lines = sys.stdin.read().split("\n")
if lines and lines[-1] == "":
    lines.pop()
for n, line in enumerate(lines, 1):
    f = line.split(" ")
    op = f[0]
    ok = False
    if op in ("deposit", "withdraw") and len(f) == 3 and amount(f[2]) is not None:
        ok = True
    elif op == "transfer" and len(f) == 4 and amount(f[3]) is not None and f[1] != f[2]:
        ok = True
    elif op in ("freeze", "unfreeze") and len(f) == 2:
        ok = True
    if not ok:
        invalid += 1
        continue
    for a in f[1:3] if op == "transfer" else f[1:2]:
        bal.setdefault(a, 0)
    a = f[1]
    touched = []
    if op == "freeze":
        frozen.add(a)
        applied += 1
    elif op == "unfreeze":
        frozen.discard(a)
        applied += 1
    elif op == "deposit":
        if a in frozen:
            rej_frozen += 1
        else:
            bal[a] += amount(f[2])
            applied += 1
            touched = [a]
    elif op == "withdraw":
        x = amount(f[2])
        if a in frozen:
            rej_frozen += 1
        elif bal[a] < x:
            rej_insuf += 1
        else:
            bal[a] -= x
            applied += 1
    else:
        b, x = f[2], amount(f[3])
        if a in frozen or b in frozen:
            rej_frozen += 1
        elif bal[a] < x:
            rej_insuf += 1
        else:
            bal[a] -= x
            bal[b] += x
            applied += 1
            touched = [b]
    for t in touched:
        if bal[t] > peak[0]:
            peak = (bal[t], t, n)

print("lines: {}".format(len(lines)))
print("applied: {}".format(applied))
print("rejected: frozen={} insufficient={}".format(rej_frozen, rej_insuf))
print("invalid: {}".format(invalid))
print("accounts: {}".format(len(bal)))
print("frozen: {}".format(len(frozen)))
print("total: {}".format(dollars(sum(bal.values()))))
print("top:")
for a, c in sorted(bal.items(), key=lambda kv: (-kv[1], kv[0]))[:5]:
    print("{} {}".format(a, dollars(c)))
print("peak: {} {} at line {}".format(peak[1], dollars(peak[0]), peak[2]))
