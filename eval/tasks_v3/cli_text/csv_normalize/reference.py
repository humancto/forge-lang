import csv
import io
import re
import sys

text = sys.stdin.buffer.read().decode("utf-8")
if text.startswith("﻿"):
    text = text[1:]
rows = list(csv.reader(io.StringIO(text, newline="")))
rows = [r for r in rows if r and r != [""]]
header = [h.strip() for h in rows[0]]
col = {h: i for i, h in enumerate(header)}
out = []
bad = []
for n, r in enumerate(rows[1:], 1):
    amount = r[col["amount"]].strip()
    m = re.fullmatch(r"(-?)(\d+)(?:\.(\d{1,2}))?", amount)
    if amount == "":
        cents = 0
    elif m:
        cents = int(m.group(2)) * 100 + int((m.group(3) or "0").ljust(2, "0"))
        if m.group(1):
            cents = -cents
    else:
        bad.append(str(n))
        continue
    notes = r[col["notes"]].replace("\r\n", "\n").replace("\n", " / ")
    out.append((r[col["id"]], r[col["name"]].strip(), cents, notes))
out.sort(key=lambda x: (-x[2], x[0]))


def field(s):
    s = str(s)
    if any(c in s for c in ',"\n\r') or s != s.strip():
        return '"' + s.replace('"', '""') + '"'
    return s


print("id,name,amount_cents,notes")
for row in out:
    print(",".join(field(x) for x in row))
print("# {} rows, total {}, bad rows: {}".format(len(out), sum(r[2] for r in out), " ".join(bad) or "none"))
