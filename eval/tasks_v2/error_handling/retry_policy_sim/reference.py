import re
import sys

lines = [l.strip() for l in sys.stdin.read().splitlines() if l.strip()]
cfg = dict(kv.split("=") for kv in lines[0].split())
max_attempts, base, factor, cap = (int(cfg[k]) for k in ("max_attempts", "base_ms", "factor",
                                                          "cap_ms"))
ok_jobs = failed = total_wait = 0
for line in lines[1:]:
    job, spec = line.split()
    outcomes = spec.split(",")
    waited = 0
    attempt = 0
    while True:
        attempt += 1
        out = outcomes[min(attempt, len(outcomes)) - 1]
        if out == "ok":
            print("{}: ok after {} attempt(s), waited {}ms".format(job, attempt, waited))
            ok_jobs += 1
            break
        status = out.split(":")[0]
        transient = status in ("timeout", "429") or re.fullmatch(r"5\d\d", status)
        if not transient:
            print("{}: permanent error {} after {} attempt(s)".format(job, status, attempt))
            failed += 1
            break
        if attempt == max_attempts:
            print("{}: gave up after {} attempts, waited {}ms".format(job, attempt, waited))
            failed += 1
            break
        delay = min(cap, base * factor ** (attempt - 1))
        if ":" in out:
            delay = max(delay, int(out.split(":")[1]))
        print("{}: attempt {} {} -> wait {}ms".format(job, attempt, out, delay))
        waited += delay
    total_wait += waited
print("succeeded={} failed={} total_wait={}ms".format(ok_jobs, failed, total_wait))
