import re
import sys
from collections import OrderedDict

caches = {"nums": (3, OrderedDict()), "names": (2, OrderedDict())}

for line in sys.stdin.read().splitlines():
    if not line.strip():
        continue
    p = line.split(" ")
    if p[0] == "sum" and len(p) == 1:
        print("sum = {}".format(sum(caches["nums"][1].values())))
        continue
    if len(p) < 2 or p[1] not in caches:
        print("error: bad command")
        continue
    name = p[1]
    cap, data = caches[name]
    if p[0] == "put" and len(p) >= 4:
        key = p[2]
        value = line.split(" ", 3)[3]
        if name == "nums":
            if not re.fullmatch(r"-?\d+", value):
                print("error: nums needs an integer")
                continue
            value = int(value)
        if key in data:
            data.move_to_end(key)
        elif len(data) >= cap:
            old, _ = data.popitem(last=False)
            print("evicted {}/{}".format(name, old))
        data[key] = value
    elif p[0] == "get" and len(p) == 3:
        if p[2] in data:
            data.move_to_end(p[2])
            print("{}/{} = {}".format(name, p[2], data[p[2]]))
        else:
            print("{}/{} miss".format(name, p[2]))
    elif p[0] == "dump" and len(p) == 2:
        print(("{}: ".format(name) + " ".join("{}={}".format(k, v) for k, v in data.items())).rstrip())
    else:
        print("error: bad command")
