shapes = [("circle", (1.5,)), ("rectangle", (2, 3.5)), ("triangle", (3, 4)),
          ("circle", (0.5,)), ("rectangle", (10, 0.25))]
def area(kind, d):
    if kind == "circle": return 3.14159 * d[0] * d[0]
    if kind == "rectangle": return d[0] * d[1]
    return d[0] * d[1] / 2
total = 0.0
for k, d in shapes:
    a = area(k, d)
    total += a
    print(f"{k}: {a:.2f}")
print(f"total: {total:.2f}")
