jobs = [("alpha", 1), ("beta", 3), ("gamma", 6), ("delta", 2)]
class Fail(Exception): pass
def fetch(job, need, attempt):
    if attempt < need:
        raise Fail(f"{job}: attempt {attempt} failed")
    return f"{job} done"
for job, need in jobs:
    print(f"== {job} ==")
    for attempt in range(1, 5):
        try:
            v = fetch(job, need, attempt)
            print(f"{job}: {v} after {attempt} attempt(s)")
            break
        except Fail as e:
            print(f"  retry: {e}")
    else:
        print(f"{job}: gave up after 4 attempts")
