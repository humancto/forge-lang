import sys
from typing import Optional
users = {1: ("Grace", None), 2: ("Alan", 1), 3: ("Ada", 1), 4: ("Linus", 2),
         5: ("Barbara", 4), 6: ("Ken", None)}
def find(uid) -> Optional[tuple]:
    return users.get(uid)
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    uid = int(line)
    u = find(uid)
    if u is None:
        print(f"{uid}: not found")
    elif u[1] is None:
        print(f"{uid}: {u[0]}, top level")
    else:
        print(f"{uid}: {u[0]}, reports to {find(u[1])[0]}")
