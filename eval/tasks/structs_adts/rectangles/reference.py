from dataclasses import dataclass
@dataclass
class Rectangle:
    name: str
    w: int
    h: int
    def area(self): return self.w * self.h
    def perimeter(self): return 2 * (self.w + self.h)
rs = [Rectangle("door", 2, 7), Rectangle("window", 4, 3), Rectangle("table", 3, 3),
      Rectangle("poster", 6, 1), Rectangle("rug", 5, 4)]
for r in sorted(rs, key=lambda r: (r.area(), r.name)):
    print(f"{r.name} {r.w}x{r.h} area={r.area()} perimeter={r.perimeter()}")
print("largest: " + max(rs, key=lambda r: r.area()).name)
print("squares: " + ", ".join(r.name for r in rs if r.w == r.h))
