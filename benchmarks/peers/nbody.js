// Port of benchmarks/vm/nbody.fg
function energy(x, y, z, vx, vy, vz, m) {
  const n = m.length;
  let e = 0.0;
  for (let i = 0; i < n; i++) {
    e = e + 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    for (let j = i + 1; j < n; j++) {
      const dx = x[i] - x[j];
      const dy = y[i] - y[j];
      const dz = z[i] - z[j];
      e = e - (m[i] * m[j]) / Math.sqrt(dx * dx + dy * dy + dz * dz);
    }
  }
  return e;
}

function simulate(steps) {
  const solarMass = 4.0 * Math.PI * Math.PI;
  const dpy = 365.24;
  const x = [0.0, 4.8414314424647209, 8.34336671824457987, 12.894369562139131, 15.3796971148509165];
  const y = [0.0, -1.16032004402742839, 4.12479856412430479, -15.1111514016986312, -25.9193146099879641];
  const z = [0.0, -0.103622044471123109, -0.403523417114321381, -0.223307578892655734, 0.179258772950371181];
  const vx = [0.0, 1.66007664274403694e-3 * dpy, -2.76742510726862411e-3 * dpy, 2.96460137564761618e-3 * dpy, 2.68067772490389322e-3 * dpy];
  const vy = [0.0, 7.69901118419740425e-3 * dpy, 4.99852801234917238e-3 * dpy, 2.3784717395948095e-3 * dpy, 1.62824170038242295e-3 * dpy];
  const vz = [0.0, -6.90460016972063023e-5 * dpy, 2.30417297573763929e-5 * dpy, -2.96589568540237556e-5 * dpy, -9.5159225451971587e-5 * dpy];
  const m = [solarMass, 9.54791938424326609e-4 * solarMass, 2.85885980666130812e-4 * solarMass, 4.36624404335156298e-5 * solarMass, 5.15138902046611451e-5 * solarMass];
  const n = m.length;

  let px = 0.0;
  let py = 0.0;
  let pz = 0.0;
  for (let i = 0; i < n; i++) {
    px = px + vx[i] * m[i];
    py = py + vy[i] * m[i];
    pz = pz + vz[i] * m[i];
  }
  vx[0] = -px / solarMass;
  vy[0] = -py / solarMass;
  vz[0] = -pz / solarMass;

  const before = energy(x, y, z, vx, vy, vz, m);
  const dt = 0.01;
  for (let s = 0; s < steps; s++) {
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const dx = x[i] - x[j];
        const dy = y[i] - y[j];
        const dz = z[i] - z[j];
        const d2 = dx * dx + dy * dy + dz * dz;
        const mag = dt / (d2 * Math.sqrt(d2));
        const mi = m[i] * mag;
        const mj = m[j] * mag;
        vx[i] = vx[i] - dx * mj;
        vy[i] = vy[i] - dy * mj;
        vz[i] = vz[i] - dz * mj;
        vx[j] = vx[j] + dx * mi;
        vy[j] = vy[j] + dy * mi;
        vz[j] = vz[j] + dz * mi;
      }
    }
    for (let i = 0; i < n; i++) {
      x[i] = x[i] + dt * vx[i];
      y[i] = y[i] + dt * vy[i];
      z[i] = z[i] + dt * vz[i];
    }
  }
  const after = energy(x, y, z, vx, vy, vz, m);
  return `${Math.round(before * 1000000000)} ${Math.round(after * 1000000000)}`;
}
console.log(simulate(20000));
