'use strict';
function fannkuch(n) {
  const perm = new Int32Array(n), perm1 = new Int32Array(n), count = new Int32Array(n);
  for (let i = 0; i < n; i++) perm1[i] = i;
  let maxFlips = 0, checksum = 0, permCount = 0, r = n;
  for (;;) {
    while (r !== 1) { count[r - 1] = r; r--; }
    for (let i = 0; i < n; i++) perm[i] = perm1[i];
    let flips = 0, k = perm[0];
    while (k !== 0) {
      let lo = 0, hi = k;
      while (lo < hi) { const t = perm[lo]; perm[lo] = perm[hi]; perm[hi] = t; lo++; hi--; }
      flips++;
      k = perm[0];
    }
    if (flips > maxFlips) maxFlips = flips;
    checksum += permCount % 2 === 0 ? flips : -flips;
    for (;;) {
      if (r === n) return [checksum, maxFlips];
      const p0 = perm1[0];
      for (let i = 0; i < r; i++) perm1[i] = perm1[i + 1];
      perm1[r] = p0;
      count[r]--;
      if (count[r] > 0) break;
      r++;
    }
    permCount++;
  }
}
const [c, m] = fannkuch(parseInt(process.argv[2], 10));
console.log('RESULT ' + c + ':' + m);
