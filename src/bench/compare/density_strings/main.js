'use strict';
const n = parseInt(process.argv[2], 10);
const all = new Array(n);
for (let i = 0; i < n; i++) all[i] = 'k' + i;
let s = 0;
for (let i = 0; i < n; i++) s += all[i].length;
console.log('RESULT ' + s);
