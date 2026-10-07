'use strict';
// worker_threads: Node's OS-thread facility (one isolate per worker).
const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');
const THREADS = 4;
if (isMainThread) {
  const n = parseInt(process.argv[2], 10);
  let total = 0, done = 0;
  for (let t = 0; t < THREADS; t++) {
    const w = new Worker(__filename, { workerData: { n, t } });
    w.on('message', (v) => { total += v; if (++done === THREADS) console.log('RESULT ' + total); });
  }
} else {
  const { n, t } = workerData;
  const chunk = Math.floor(n / THREADS);
  const lo = t * chunk, hi = t === THREADS - 1 ? n : lo + chunk;
  let acc = 0;
  for (let i = lo; i < hi; i++) acc += (i * i) % 7;
  parentPort.postMessage(acc);
}
