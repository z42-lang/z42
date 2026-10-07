'use strict';
const size = parseInt(process.argv[2], 10);
let inside = 0;
for (let y = 0; y < size; y++) {
  const ci = 2.0 * y / size - 1.0;
  for (let x = 0; x < size; x++) {
    const cr = 2.0 * x / size - 1.5;
    let zr = 0.0, zi = 0.0, escaped = false;
    for (let it = 0; it < 50; it++) {
      const tr = zr * zr - zi * zi + cr;
      zi = 2.0 * zr * zi + ci;
      zr = tr;
      if (zr * zr + zi * zi > 4.0) { escaped = true; break; }
    }
    if (!escaped) inside++;
  }
}
console.log('RESULT ' + inside);
