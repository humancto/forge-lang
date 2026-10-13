# Port of benchmarks/vm/records_pipeline.fg
from functools import reduce

rows = list(
    map(
        lambda i: {"id": i, "price": (i * 7919) % 1000, "qty": (i * 31) % 7},
        range(0, 300000),
    )
)
big = list(filter(lambda r: r["qty"] > 2, rows))
totals = list(map(lambda r: r["price"] * r["qty"], big))
print(len(totals))
print(reduce(lambda acc, t: acc + t, totals, 0))
