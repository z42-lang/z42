'use strict';
const n = parseInt(process.argv[2], 10);
const list = [];
for (let i = 0; i < n; i++) list.push(i % 1000);
let acc = 0;
for (let p = 0; p < 10; p++) for (let i = 0; i < list.length; i++) acc += list[i] ^ p;
console.log('RESULT ' + acc);
