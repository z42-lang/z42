'use strict';
// JS has no StringBuilder; `+=` on a rope-backed string is the idiomatic builder.
const n = parseInt(process.argv[2], 10);
let s = '';
for (let i = 0; i < n; i++) { s += 'item'; s += String(i); s += ';'; }
let sevens = 0;
for (let i = 0; i < s.length; i++) if (s.charCodeAt(i) === 55) sevens++;
console.log('RESULT ' + (s.length + sevens));
