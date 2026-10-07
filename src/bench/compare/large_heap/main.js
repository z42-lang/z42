'use strict';
class LNode { constructor() { this.id = 0; this.data = null; this.next = null; } }
function makeChain(first, chain) {
  let head = null;
  for (let k = 0; k < chain; k++) {
    const n = new LNode();
    n.id = first + k;
    const d = [0, 0, 0, 0, 0, 0, 0, 0];
    d[0] = n.id; d[7] = n.id * 3;
    n.data = d; n.next = head; head = n;
  }
  return head;
}
function chainSum(h) { let s = 0; for (; h !== null; h = h.next) s += h.data[0] + h.data[7]; return s; }
function drive(slots) {
  const chain = 8, churn = slots * 8;
  const table = new Array(slots);
  let nextId = 0;
  for (let s = 0; s < slots; s++) { table[s] = makeChain(nextId, chain); nextId += chain; }
  let seed = 12345, acc = 0;
  for (let i = 0; i < churn; i++) {
    // (seed * 1103515245 + 12345) mod 2^31 — the low 31 bits of the 32-bit product are exact.
    seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
    const slot = seed % slots;
    acc += chainSum(table[slot]);
    table[slot] = makeChain(nextId, chain);
    nextId += chain;
    const t = new LNode(); t.id = i;
    const tmp = [0, 0, 0, 0]; tmp[1] = i;
    acc += tmp[1] % 7 + t.id % 5;
  }
  for (let s = 0; s < slots; s++) acc += chainSum(table[s]);
  return acc;
}
console.log('RESULT ' + drive(parseInt(process.argv[2], 10)));
