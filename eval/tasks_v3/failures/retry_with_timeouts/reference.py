import os
import sys
import threading
import time

LIMIT = 0.15


def attempt(spec):
    if spec == "x":
        return "error"
    done = threading.Event()

    def work():
        time.sleep(int(spec) / 1000)
        done.set()

    threading.Thread(target=work, daemon=True).start()
    return "ok" if done.wait(LIMIT) else "timeout"


for line in sys.stdin.read().split("\n"):
    f = line.strip().split(" ")
    if len(f) != 3:
        continue
    name, specs, max_attempts = f[0], f[1].split(","), int(f[2])
    outcomes = []
    for spec in specs[:max_attempts]:
        outcomes.append(attempt(spec))
        if outcomes[-1] == "ok":
            break
    if outcomes[-1] == "ok":
        print("{}: ok after {} attempts ({})".format(name, len(outcomes), ", ".join(outcomes)))
    else:
        print("{}: gave up after {} attempts ({})".format(name, len(outcomes), ", ".join(outcomes)))
sys.stdout.flush()
os._exit(0)
