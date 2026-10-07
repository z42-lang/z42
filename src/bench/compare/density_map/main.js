'use strict';
const n = parseInt(process.argv[2], 10);
const d = new Map();
for (let i = 0; i < n; i++) d.set('k' + i, i);
let s = d.size;
for (let i = 0; i < n; i += 7919) s += d.get('k' + i);
console.log('RESULT ' + s);
