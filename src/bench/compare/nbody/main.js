'use strict';
const SOLAR_MASS = 39.47841760435743;
const DAYS = 365.24;

class Body {
  constructor(x, y, z, vx, vy, vz, mass) {
    this.x = x; this.y = y; this.z = z;
    this.vx = vx * DAYS; this.vy = vy * DAYS; this.vz = vz * DAYS;
    this.mass = mass * SOLAR_MASS;
  }
}

function makeSystem() {
  const b = [
    new Body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
    new Body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
      1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
    new Body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
      -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
    new Body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
      2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
    new Body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
      2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
  ];
  let px = 0.0, py = 0.0, pz = 0.0;
  for (const o of b) { px = px + o.vx * o.mass; py = py + o.vy * o.mass; pz = pz + o.vz * o.mass; }
  b[0].vx = -px / SOLAR_MASS; b[0].vy = -py / SOLAR_MASS; b[0].vz = -pz / SOLAR_MASS;
  return b;
}

function advance(b, dt) {
  const n = b.length;
  for (let i = 0; i < n; i++) {
    const bi = b[i];
    for (let j = i + 1; j < n; j++) {
      const bj = b[j];
      const dx = bi.x - bj.x, dy = bi.y - bj.y, dz = bi.z - bj.z;
      const d2 = dx * dx + dy * dy + dz * dz;
      const mag = dt / (d2 * Math.sqrt(d2));
      const mi = bi.mass * mag, mj = bj.mass * mag;
      bi.vx = bi.vx - dx * mj; bi.vy = bi.vy - dy * mj; bi.vz = bi.vz - dz * mj;
      bj.vx = bj.vx + dx * mi; bj.vy = bj.vy + dy * mi; bj.vz = bj.vz + dz * mi;
    }
  }
  for (let i = 0; i < n; i++) {
    const bi = b[i];
    bi.x = bi.x + dt * bi.vx; bi.y = bi.y + dt * bi.vy; bi.z = bi.z + dt * bi.vz;
  }
}

function energy(b) {
  let e = 0.0;
  const n = b.length;
  for (let i = 0; i < n; i++) {
    const bi = b[i];
    e = e + 0.5 * bi.mass * (bi.vx * bi.vx + bi.vy * bi.vy + bi.vz * bi.vz);
    for (let j = i + 1; j < n; j++) {
      const bj = b[j];
      const dx = bi.x - bj.x, dy = bi.y - bj.y, dz = bi.z - bj.z;
      e = e - bi.mass * bj.mass / Math.sqrt(dx * dx + dy * dy + dz * dz);
    }
  }
  return e;
}

const b = makeSystem();
const steps = parseInt(process.argv[2], 10);
for (let s = 0; s < steps; s++) advance(b, 0.01);
console.log('RESULT ' + Math.trunc(energy(b) * 1000000000.0));
