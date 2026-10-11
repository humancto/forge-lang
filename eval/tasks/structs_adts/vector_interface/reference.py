from abc import ABC, abstractmethod
class Describable(ABC):
    @abstractmethod
    def describe(self) -> str: ...
class Vec(Describable):
    def __init__(self, x, y): self.x, self.y = x, y
    def add(self, o): return Vec(self.x + o.x, self.y + o.y)
    def scale(self, k): return Vec(self.x * k, self.y * k)
    def dot(self, o): return self.x * o.x + self.y * o.y
    def len2(self): return self.dot(self)
    def describe(self): return f"Vec({self.x}, {self.y})"
a, b = Vec(3, -4), Vec(1, 2)
print(f"a = {a.describe()}")
print(f"b = {b.describe()}")
print(f"a + b = {a.add(b).describe()}")
print(f"3 * b = {b.scale(3).describe()}")
print(f"a . b = {a.dot(b)}")
print(f"|a|^2 = {a.len2()}")
print(f"a + 2b = {a.add(b.scale(2)).describe()}")
