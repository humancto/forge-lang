calls = 0
def memoize(f):
    cache = {}
    def g(n):
        if n not in cache:
            cache[n] = f(n)
        return cache[n]
    return g
@memoize
def fib(n):
    global calls
    calls += 1
    return n if n < 2 else fib(n - 1) + fib(n - 2)
for n in (10, 50, 90):
    print(f"fib({n}) = {fib(n)}")
print(f"calls: {calls}")
