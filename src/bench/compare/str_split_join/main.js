'use strict';
const rounds = parseInt(process.argv[2], 10);
const fields = [];
for (let i = 0; i < 200; i++) fields.push('f' + (i * 37));
const line = fields.join(',');
let acc = 0;
for (let r = 0; r < rounds; r++) {
  const parts = line.split(',');
  const joined = parts.join(';');
  acc += joined.length + parts.length;
}
console.log('RESULT ' + acc);
