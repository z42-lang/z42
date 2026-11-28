import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const [modulePath, libs, zbc, withCompression, withProfiler] = process.argv.slice(2);
function readFile(name, kind) {
  try {
    const dir = kind === 3 || kind === 4 ? path.dirname(zbc) : libs;
    return fs.readFileSync(path.join(dir, name));
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}
if (withCompression === 'default') {
  const { Z42VM } = await import(pathToFileURL(modulePath));
  const vm = new Z42VM({ fileResolver: readFile });
  const module = vm.loadBytes(new Uint8Array(fs.readFileSync(zbc)), path.basename(zbc));
  assert.equal(vm.invoke(vm.resolveEntry(module, 'EngineComponentProbe.Echo'), [40]), 41);
  assert.equal(vm.invoke(vm.resolveEntry(module, 'EngineComponentProbe.CompressionRoundtrip'), []), 36);
  assert.throws(() => vm.invoke(vm.resolveEntry(module, 'EngineComponentProbe.HeapInspection'), []), /profiler/);
  vm.dispose();
  console.log('default final WASM: interpreter execution + bundled compression passed');
  process.exit(0);
}
let exports;
const decoder = new TextDecoder();
const string = pointer => {
  const bytes = new Uint8Array(exports.memory.buffer);
  let end = pointer;
  while (bytes[end]) end++;
  return decoder.decode(bytes.subarray(pointer, end));
};
const errors = [];
({ instance: { exports } } = await WebAssembly.instantiate(fs.readFileSync(modulePath), { env: {
  resolve(pointer, kind, outBytes, outLength) {
    const bytes = readFile(string(pointer), kind);
    if (!bytes) return 0;
    const file = store(bytes);
    const memory = new DataView(exports.memory.buffer);
    memory.setUint32(outBytes, file.pointer, true);
    memory.setUint32(outLength, file.length, true);
    return 1;
  },
  report(pointer) { errors.push(string(pointer)); },
} }));
let cursor = exports.input();
const limit = cursor + 64 * 1024 * 1024;
function store(bytes) {
  assert(cursor + bytes.length <= limit, 'fixture data exceeds reserved input arena');
  const pointer = cursor; cursor = (cursor + bytes.length + 7) & ~7;
  new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes);
  return { pointer, length: bytes.length };
}
const name = store(Buffer.from(path.basename(zbc) + '\0'));
const code = store(fs.readFileSync(zbc));
assert.equal(exports.run(name.pointer, code.pointer, code.length), 0, errors.join('\n'));
if (withCompression === 'false') assert(errors.some(error => error.includes('compression component unavailable')), errors.join('\n'));
if (withProfiler === 'false') assert(errors.some(error => error.includes('profiler')), errors.join('\n'));
assert(errors.every(error => (withCompression === 'false' && error.includes('compression')) || (withProfiler === 'false' && error.includes('profiler'))), errors.join('\n'));
console.log(`relinked WASM: interpreter execution + compression ${withCompression === 'true' ? 'roundtrip' : 'omission'} passed`);
