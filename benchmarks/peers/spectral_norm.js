// Port of benchmarks/vm/spectral_norm.fg
function a(i, j) {
  return 1.0 / (((i + j) * (i + j + 1)) / 2 + i + 1);
}

function mulAv(v, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    let s = 0.0;
    for (let j = 0; j < n; j++) {
      s = s + a(i, j) * v[j];
    }
    out.push(s);
  }
  return out;
}

function mulAtv(v, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    let s = 0.0;
    for (let j = 0; j < n; j++) {
      s = s + a(j, i) * v[j];
    }
    out.push(s);
  }
  return out;
}

function spectralNorm(n) {
  let u = [];
  for (let i = 0; i < n; i++) {
    u.push(1.0);
  }
  let v = [];
  for (let k = 0; k < 10; k++) {
    v = mulAtv(mulAv(u, n), n);
    u = mulAtv(mulAv(v, n), n);
  }
  let vbv = 0.0;
  let vv = 0.0;
  for (let i = 0; i < n; i++) {
    vbv = vbv + u[i] * v[i];
    vv = vv + v[i] * v[i];
  }
  return Math.sqrt(vbv / vv);
}
console.log(Math.round(spectralNorm(100) * 1000000000));
