import re
import sys


class Fail(Exception):
    pass


def amount(s, allow_zero=False):
    if not re.fullmatch(r"\d+", s) or (int(s) == 0 and not allow_zero):
        raise Fail("invalid amount")
    return int(s)


def need(acc, name):
    if name not in acc:
        raise Fail("unknown account " + name)


def apply(acc, line):
    p = line.split()
    cmd = p[0]
    if cmd == "open" and len(p) == 3:
        if p[1] in acc:
            raise Fail("account exists")
        acc[p[1]] = amount(p[2], allow_zero=True)
    elif cmd in ("deposit", "withdraw") and len(p) == 3:
        need(acc, p[1])
        a = amount(p[2])
        if cmd == "withdraw":
            if acc[p[1]] < a:
                raise Fail("insufficient funds")
            a = -a
        acc[p[1]] += a
    elif cmd == "transfer" and len(p) == 4:
        need(acc, p[1])
        need(acc, p[2])
        if p[1] == p[2]:
            raise Fail("same account")
        a = amount(p[3])
        if acc[p[1]] < a:
            raise Fail("insufficient funds")
        acc[p[1]] -= a
        acc[p[2]] += a
    else:
        raise Fail("unknown command")


accounts = {}
failed = False
batch = None
nbatch = 0
for raw in sys.stdin.read().splitlines():
    line = raw.strip()
    if not line:
        continue
    if line == "batch":
        batch = []
        continue
    if line == "end":
        nbatch += 1
        trial = dict(accounts)
        try:
            for i, cmd in enumerate(batch, 1):
                try:
                    apply(trial, cmd)
                except Fail as e:
                    raise Fail("failed at command {}: {}".format(i, e))
            accounts = trial
            print("batch {}: ok ({} commands)".format(nbatch, len(batch)))
        except Fail as e:
            failed = True
            print("batch {}: {}".format(nbatch, e))
        batch = None
        continue
    if batch is not None:
        batch.append(line)
        continue
    try:
        apply(accounts, line)
        print("{}: ok".format(line))
    except Fail as e:
        failed = True
        print("{}: error {}".format(line, e))
for name in sorted(accounts):
    print("{} {}".format(name, accounts[name]))
print("total {}".format(sum(accounts.values())))
sys.exit(1 if failed else 0)
