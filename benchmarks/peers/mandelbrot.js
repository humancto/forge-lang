// Port of benchmarks/vm/mandelbrot.fg
function mandelbrot(size, maxIter) {
  let inside = 0;
  for (let py = 0; py < size; py++) {
    const ci = (2.0 * py) / size - 1.0;
    for (let px = 0; px < size; px++) {
      const cr = (2.0 * px) / size - 1.5;
      let zr = 0.0;
      let zi = 0.0;
      let i = 0;
      while (i < maxIter) {
        const tr = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci;
        zr = tr;
        if (zr * zr + zi * zi > 4.0) {
          break;
        }
        i = i + 1;
      }
      if (i === maxIter) {
        inside = inside + 1;
      }
    }
  }
  return inside;
}
console.log(mandelbrot(200, 50));
