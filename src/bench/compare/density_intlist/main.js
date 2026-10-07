'use strict';
const n = parseInt(process.argv[2], 10);
const lst = [];
for (let i = 0; i < n; i++) lst.push(i);
let s = 0;
for (let i = 0; i < lst.length; i++) s += lst[i];
console.log('RESULT ' + s);
