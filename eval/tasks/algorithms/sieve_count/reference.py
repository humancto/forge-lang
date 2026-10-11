n = int(input())
s = bytearray([1]) * n
s[0] = s[1] = 0
i = 2
while i * i < n:
    if s[i]:
        s[i*i::i] = bytearray(len(s[i*i::i]))
    i += 1
ps = [i for i in range(n) if s[i]]
print(f"primes below {n}: {len(ps)}")
print(f"largest: {ps[-1]}")
print(f"sum: {sum(ps)}")
print(f"twin pairs: {sum(1 for a, b in zip(ps, ps[1:]) if b - a == 2)}")
