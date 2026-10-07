'use strict';
function record(i) {
  return { id: i, name: 'user' + i, active: i % 3 === 0, tags: ['t' + (i % 10), 'x'], pos: { x: i % 100, y: i % 37 } };
}
const n = parseInt(process.argv[2], 10);
const root = [];
for (let i = 0; i < n; i++) root.push(record(i));
const text = JSON.stringify(root);
const back = JSON.parse(text);
let acc = text.length;
for (const r of back) { acc += r.id + r.pos.y; if (r.active) acc += 1; }
console.log('RESULT ' + acc);
