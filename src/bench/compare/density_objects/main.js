'use strict';
class Node { constructor(v, next) { this.v = v; this.next = next; } }
const n = parseInt(process.argv[2], 10);
let head = null;
for (let i = 0; i < n; i++) head = new Node(i, head);
let s = 0;
for (let p = head; p !== null; p = p.next) s += p.v;
console.log('RESULT ' + s);
