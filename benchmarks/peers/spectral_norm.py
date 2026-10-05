# Port of benchmarks/vm/spectral_norm.fg
import math


def a(i, j):
    return 1.0 / ((i + j) * (i + j + 1) // 2 + i + 1)


def mul_av(v, n):
    out = []
    for i in range(n):
        s = 0.0
        for j in range(n):
            s = s + a(i, j) * v[j]
        out.append(s)
    return out


def mul_atv(v, n):
    out = []
    for i in range(n):
        s = 0.0
        for j in range(n):
            s = s + a(j, i) * v[j]
        out.append(s)
    return out


def spectral_norm(n):
    u = []
    for i in range(n):
        u.append(1.0)
    v = []
    for k in range(10):
        v = mul_atv(mul_av(u, n), n)
        u = mul_atv(mul_av(v, n), n)
    vbv = 0.0
    vv = 0.0
    for i in range(n):
        vbv = vbv + u[i] * v[i]
        vv = vv + v[i] * v[i]
    return math.sqrt(vbv / vv)


print(round(spectral_norm(100) * 1000000000))
