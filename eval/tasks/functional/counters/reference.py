def make_counter(start, step):
    cur = start
    def nxt():
        nonlocal cur
        v = cur
        cur += step
        return v
    return nxt
cs = {"c1": make_counter(0, 1), "c2": make_counter(100, -10), "c3": make_counter(5, 5)}
for n in ["c1", "c1", "c2", "c3", "c1", "c2", "c2", "c3"]:
    print(f"{n}: {cs[n]()}")
print(f"c1 next: {cs['c1']()}")
