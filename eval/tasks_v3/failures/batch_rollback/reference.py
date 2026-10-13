import os
import re
import sqlite3
import sys

if os.path.exists("ledger.db"):
    os.remove("ledger.db")
db = sqlite3.connect("ledger.db", isolation_level=None)
db.execute("CREATE TABLE accounts (name TEXT PRIMARY KEY, balance INTEGER NOT NULL CHECK (balance >= 0))")


class OpError(Exception):
    pass


def apply(words):
    op = words[0]
    if op == "open" and len(words) == 3 and re.fullmatch(r"[0-9]+", words[2]):
        if db.execute("SELECT 1 FROM accounts WHERE name = ?", (words[1],)).fetchone():
            raise OpError("account {} exists".format(words[1]))
        db.execute("INSERT INTO accounts VALUES (?, ?)", (words[1], int(words[2])))
        return
    if op in ("deposit", "withdraw") and len(words) == 3:
        name, amount = words[1], words[2]
        names = [name]
    elif op == "transfer" and len(words) == 4:
        names, amount = [words[1], words[2]], words[3]
    else:
        raise OpError("bad operation")
    if not re.fullmatch(r"[0-9]+", amount) or int(amount) == 0:
        raise OpError("invalid amount")
    for n in names:
        if not db.execute("SELECT 1 FROM accounts WHERE name = ?", (n,)).fetchone():
            raise OpError("unknown account {}".format(n))
    amount = int(amount)
    if op == "deposit":
        db.execute("UPDATE accounts SET balance = balance + ? WHERE name = ?", (amount, names[0]))
        return
    src = names[0]
    bal = db.execute("SELECT balance FROM accounts WHERE name = ?", (src,)).fetchone()[0]
    if bal < amount:
        raise OpError("insufficient funds in {}".format(src))
    db.execute("UPDATE accounts SET balance = balance - ? WHERE name = ?", (amount, src))
    if op == "transfer":
        db.execute("UPDATE accounts SET balance = balance + ? WHERE name = ?", (amount, names[1]))


batch = None  # (number, start line, ops)
batches = 0
for n, line in enumerate(sys.stdin.read().split("\n"), 1):
    words = line.split()
    if not words or words[0].startswith("#"):
        continue
    if words == ["begin"]:
        batches += 1
        batch = [batches, n, 0, None]
        db.execute("BEGIN")
        continue
    if words == ["end"]:
        if batch[3] is None:
            db.execute("COMMIT")
            print("batch {}: committed {} operations".format(batch[0], batch[2]))
        else:
            db.execute("ROLLBACK")
            print("batch {}: rolled back ({} at line {})".format(batch[0], batch[3][1], batch[3][0]))
        batch = None
        continue
    if batch is not None:
        if batch[3] is None:
            try:
                apply(words)
                batch[2] += 1
            except OpError as e:
                batch[3] = (n, str(e))
        continue
    try:
        db.execute("BEGIN")
        apply(words)
        db.execute("COMMIT")
        print("line {}: ok".format(n))
    except OpError as e:
        db.execute("ROLLBACK")
        print("line {}: {}".format(n, e))

rows = db.execute("SELECT name, balance FROM accounts ORDER BY name").fetchall()
for name, bal in rows:
    print("{} {}".format(name, bal))
print("total {}".format(sum(b for _, b in rows)))
