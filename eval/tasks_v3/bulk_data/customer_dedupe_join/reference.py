import sys


def dollars(c):
    return "{}.{:02d}".format(c // 100, c % 100)


def normalize(email):
    e = email.strip().lower()
    at = e.rfind("@")
    local, dom = e[:at], e[at + 1:]
    plus = local.find("+")
    if plus >= 0:
        local = local[:plus]
    if dom in ("gmail.com", "googlemail.com"):
        local = local.replace(".", "")
        dom = "gmail.com"
    return local + "@" + dom


section = None
canon_of_email = {}
info = {}
canon = {}
n_customers = 0
orders = orphans = 0
stats = {}
months = {}
for line in sys.stdin.read().split("\n"):
    if not line.strip():
        continue
    if line.startswith("#"):
        section = line[1:].strip()
        continue
    f = line.split(",")
    if section == "customers":
        n_customers += 1
        cid, name, email = f
        key = normalize(email)
        info[cid] = (name, key)
        if key not in canon_of_email or cid < canon_of_email[key]:
            canon_of_email[key] = cid
    else:
        orders += 1
        oid, cid, cents, date = f
        if cid not in info:
            orphans += 1
            continue
        c = canon_of_email[info[cid][1]]
        cents = int(cents)
        s = stats.setdefault(c, [0, 0, date, date])
        s[0] += 1
        s[1] += cents
        s[2] = min(s[2], date)
        s[3] = max(s[3], date)
        if date.startswith("2026-"):
            m = months.setdefault(date[:7], [0, 0])
            m[0] += cents
            m[1] += 1

print("customers: {}".format(n_customers))
print("people: {}".format(len(canon_of_email)))
print("merged: {}".format(n_customers - len(canon_of_email)))
print("orders: {}".format(orders))
print("orphans: {}".format(orphans))
print("top spenders:")
for c, s in sorted(stats.items(), key=lambda kv: (-kv[1][1], kv[0]))[:10]:
    print("{} {} {} orders={} total={} first={} last={}".format(
        c, info[c][0], info[c][1], s[0], dollars(s[1]), s[2], s[3]))
print("2026 by month:")
for m in sorted(months):
    print("{} {} ({})".format(m, dollars(months[m][0]), months[m][1]))
