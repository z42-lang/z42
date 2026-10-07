'use strict';
function a(i, j) { const ij = i + j; return 1.0 / ((ij * (ij + 1)) / 2 + i + 1); }
function mulAv(v, av, n) {
  for (let i = 0; i < n; i++) { let s = 0.0; for (let j = 0; j < n; j++) s = s + a(i, j) * v[j]; av[i] = s; }
}
function mulAtv(v, atv, n) {
  for (let i = 0; i < n; i++) { let s = 0.0; for (let j = 0; j < n; j++) s = s + a(j, i) * v[j]; atv[i] = s; }
}
function mulAtAv(v, dst, tmp, n) { mulAv(v, tmp, n); mulAtv(tmp, dst, n); }

const n = parseInt(process.argv[2], 10);
const u = new Float64Array(n).fill(1.0), v = new Float64Array(n), tmp = new Float64Array(n);
for (let k = 0; k < 10; k++) { mulAtAv(u, v, tmp, n); mulAtAv(v, u, tmp, n); }
let vbv = 0.0, vv = 0.0;
for (let i = 0; i < n; i++) { vbv = vbv + u[i] * v[i]; vv = vv + v[i] * v[i]; }
console.log('RESULT ' + Math.trunc(Math.sqrt(vbv / vv) * 1000000000.0));
