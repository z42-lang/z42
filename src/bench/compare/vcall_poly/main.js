'use strict';
class Shape { area(k) { return 0; } }
class Square extends Shape { area(k) { return k * k; } }
class Rect extends Shape { area(k) { return k * 2; } }
class Tri extends Shape { area(k) { return Math.trunc(k / 2); } }
class Dot extends Shape { area(k) { return 1; } }
function drive(shapes, n) {
  let acc = 0;
  for (let i = 0; i < n; i++) acc += shapes[i % 4].area(i % 1000);
  return acc;
}
console.log('RESULT ' + drive([new Square(), new Rect(), new Tri(), new Dot()], parseInt(process.argv[2], 10)));
