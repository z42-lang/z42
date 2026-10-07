'use strict';
function apply(f, x) { return f(x); }
function main(n) {
  let acc = 0;
  for (let i = 0; i < n; i++) {
    const k = i % 7;
    const f = (x) => x * 3 + k;
    acc += apply(f, i);
  }
  return acc;
}
console.log('RESULT ' + main(parseInt(process.argv[2], 10)));
