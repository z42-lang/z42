'use strict';
const n = parseInt(process.argv[2], 10);
let acc = 0;
for (let i = 0; i < n; i++) {
  const v = i * 7919 + 1;
  const s = String(v);
  acc += s.length + parseInt(s, 10) % 7;
}
console.log('RESULT ' + acc);
