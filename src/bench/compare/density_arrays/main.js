'use strict';
// A plain array of 8 small ints (SMIs) — the idiomatic JS "array of 8 integers".
const n = parseInt(process.argv[2], 10);
const all = new Array(n);
for (let i = 0; i < n; i++) {
  const a = [0, 0, 0, 0, 0, 0, 0, 0];
  a[0] = i; a[7] = i * 3;
  all[i] = a;
}
let s = 0;
for (let i = 0; i < n; i++) s += all[i][0] + all[i][7];
console.log('RESULT ' + s);
