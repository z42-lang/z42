'use strict';
class TreeNode { constructor(l, r) { this.left = l; this.right = r; } }
function bottom(d) { return d > 0 ? new TreeNode(bottom(d - 1), bottom(d - 1)) : new TreeNode(null, null); }
function check(t) { return t.left === null ? 1 : 1 + check(t.left) + check(t.right); }
const maxDepth = Math.max(6, parseInt(process.argv[2], 10));
const stretch = maxDepth + 1;
console.log(`stretch tree of depth ${stretch}\t check: ${check(bottom(stretch))}`);
const longLived = bottom(maxDepth);
let total = 0;
for (let d = 4; d <= maxDepth; d += 2) {
  const iters = 1 << (maxDepth - d + 4);
  let c = 0;
  for (let i = 0; i < iters; i++) c += check(bottom(d));
  console.log(`${iters}\t trees of depth ${d}\t check: ${c}`);
  total += c;
}
const ll = check(longLived);
console.log(`long lived tree of depth ${maxDepth}\t check: ${ll}`);
console.log('RESULT ' + (total + ll));
