# Port of benchmarks/vm/mandelbrot.fg
def mandelbrot(size, max_iter):
    inside = 0
    for py in range(size):
        ci = 2.0 * py / size - 1.0
        for px in range(size):
            cr = 2.0 * px / size - 1.5
            zr = 0.0
            zi = 0.0
            i = 0
            while i < max_iter:
                tr = zr * zr - zi * zi + cr
                zi = 2.0 * zr * zi + ci
                zr = tr
                if zr * zr + zi * zi > 4.0:
                    break
                i = i + 1
            if i == max_iter:
                inside = inside + 1
    return inside


print(mandelbrot(200, 50))
