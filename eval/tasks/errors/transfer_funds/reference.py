bal = {"alice": 500, "bob": 120, "carol": 0}
ts = [("alice", "bob", 200), ("bob", "carol", 400), ("carol", "alice", 50), ("dave", "alice", 10),
      ("bob", "bob", 5), ("alice", "carol", -20), ("bob", "carol", 320), ("alice", "carol", 300)]
def transfer(f, t, a):
    if a <= 0: return "amount must be positive"
    for n in (f, t):
        if n not in bal: return f"unknown account: {n}"
    if f == t: return "cannot transfer to self"
    if bal[f] < a: return f"insufficient funds in {f}: has {bal[f]}, needs {a}"
    bal[f] -= a
    bal[t] += a
    return None
for f, t, a in ts:
    e = transfer(f, t, a)
    print(f"ok: {f} -> {t} {a}" if e is None else f"failed: {f} -> {t} {a}: {e}")
for n in sorted(bal):
    print(f"{n}: {bal[n]}")
