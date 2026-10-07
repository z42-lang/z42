'use strict';
class Op { step(x) { return x; } }
class AddOne extends Op { step(x) { return x + 1; } }
function drive(op, n) { let acc = 0; for (let i = 0; i < n; i++) acc += op.step(i); return acc; }
console.log('RESULT ' + drive(new AddOne(), parseInt(process.argv[2], 10)));
