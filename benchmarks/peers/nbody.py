# Port of benchmarks/vm/nbody.fg
import math


def energy(x, y, z, vx, vy, vz, m):
    n = len(m)
    e = 0.0
    for i in range(n):
        e = e + 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i])
        for j in range(i + 1, n):
            dx = x[i] - x[j]
            dy = y[i] - y[j]
            dz = z[i] - z[j]
            e = e - m[i] * m[j] / math.sqrt(dx * dx + dy * dy + dz * dz)
    return e


def simulate(steps):
    solar_mass = 4.0 * math.pi * math.pi
    dpy = 365.24
    x = [0.0, 4.84143144246472090e+00, 8.34336671824457987e+00, 1.28943695621391310e+01, 1.53796971148509165e+01]
    y = [0.0, -1.16032004402742839e+00, 4.12479856412430479e+00, -1.51111514016986312e+01, -2.59193146099879641e+01]
    z = [0.0, -1.03622044471123109e-01, -4.03523417114321381e-01, -2.23307578892655734e-01, 1.79258772950371181e-01]
    vx = [0.0, 1.66007664274403694e-03 * dpy, -2.76742510726862411e-03 * dpy, 2.96460137564761618e-03 * dpy, 2.68067772490389322e-03 * dpy]
    vy = [0.0, 7.69901118419740425e-03 * dpy, 4.99852801234917238e-03 * dpy, 2.37847173959480950e-03 * dpy, 1.62824170038242295e-03 * dpy]
    vz = [0.0, -6.90460016972063023e-05 * dpy, 2.30417297573763929e-05 * dpy, -2.96589568540237556e-05 * dpy, -9.51592254519715870e-05 * dpy]
    m = [solar_mass, 9.54791938424326609e-04 * solar_mass, 2.85885980666130812e-04 * solar_mass, 4.36624404335156298e-05 * solar_mass, 5.15138902046611451e-05 * solar_mass]
    n = len(m)

    px = 0.0
    py = 0.0
    pz = 0.0
    for i in range(n):
        px = px + vx[i] * m[i]
        py = py + vy[i] * m[i]
        pz = pz + vz[i] * m[i]
    vx[0] = -px / solar_mass
    vy[0] = -py / solar_mass
    vz[0] = -pz / solar_mass

    before = energy(x, y, z, vx, vy, vz, m)
    dt = 0.01
    for _ in range(steps):
        for i in range(n):
            for j in range(i + 1, n):
                dx = x[i] - x[j]
                dy = y[i] - y[j]
                dz = z[i] - z[j]
                d2 = dx * dx + dy * dy + dz * dz
                mag = dt / (d2 * math.sqrt(d2))
                mi = m[i] * mag
                mj = m[j] * mag
                vx[i] = vx[i] - dx * mj
                vy[i] = vy[i] - dy * mj
                vz[i] = vz[i] - dz * mj
                vx[j] = vx[j] + dx * mi
                vy[j] = vy[j] + dy * mi
                vz[j] = vz[j] + dz * mi
        for i in range(n):
            x[i] = x[i] + dt * vx[i]
            y[i] = y[i] + dt * vy[i]
            z[i] = z[i] + dt * vz[i]
    after = energy(x, y, z, vx, vy, vz, m)
    return "%d %d" % (round(before * 1000000000), round(after * 1000000000))


print(simulate(20000))
