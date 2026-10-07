'use strict';
const n = parseInt(process.argv[2], 10);
const d = new Map();
for (let i = 0; i < n; i++) d.set(i * 2, i);
let acc = 0;
for (let i = 0; i < 2 * n; i++) { if (d.has(i)) acc += d.get(i); }
for (let i = 0; i < n; i += 2) d.set(i * 2, 1);
acc += d.size + d.get(0) + d.get(6);
console.log('RESULT ' + acc);
