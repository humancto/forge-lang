data = [1, [2, [3, [4, 5]], 6], [], [[7], 8], [[[9]]], 10]
def flat(x):
    out = []
    for v in x:
        if isinstance(v, list):
            out.extend(flat(v))
        else:
            out.append(v)
    return out
def depth(x):
    if not isinstance(x, list):
        return 0
    return 1 + max((depth(v) for v in x), default=0)
print(" ".join(map(str, flat(data))))
print(f"depth: {depth(data)}")
