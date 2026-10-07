'use strict';
const n = parseInt(process.argv[2], 10);
const a = [];
let seed = 42;
for (let i = 0; i < n; i++) {
  // (seed * 1103515245 + 12345) mod 2^31 — the low 31 bits of the 32-bit product are exact.
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  a.push(seed % 1000000);
}
a.sort((x, y) => x - y);
let acc = 0;
for (let i = 0; i < n; i++) acc += a[i] * (i % 1000);
console.log('RESULT ' + acc);
