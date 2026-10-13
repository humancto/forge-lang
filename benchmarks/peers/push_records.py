# Port of benchmarks/vm/push_records.fg
def build(n):
    recs = []
    for i in range(0, n):
        recs.append({"id": i, "name": "r" + str(i)})
    return recs


print(len(build(100000)))
