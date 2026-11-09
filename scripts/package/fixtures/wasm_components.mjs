import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const [modulePath, libs, zbc, withCompression] = process.argv.slice(2);
if (withCompression === 'default') {
  const { Z42VM, readNamespaces } = await import(pathToFileURL(modulePath));
  const byNamespace = new Map();
  for (const file of fs.readdirSync(libs).filter(file => file.endsWith('.zpkg'))) {
    const bytes = new Uint8Array(fs.readFileSync(path.join(libs, file)));
    for (const namespace of readNamespaces(bytes)) byNamespace.set(namespace, bytes);
  }
  const vm = new Z42VM({ zpkgResolver: name => byNamespace.get(name) ?? null });
  const module = vm.loadZbc(new Uint8Array(fs.readFileSync(zbc)));
  assert.equal(vm.invoke(vm.resolveEntry(module, 'EngineComponentProbe.Echo'), [40]), 41);
  assert.equal(vm.invoke(vm.resolveEntry(module, 'EngineComponentProbe.CompressionRoundtrip'), []), 36);
  vm.dispose();
  console.log('default final WASM: interpreter execution + bundled compression passed');
  process.exit(0);
}
let exports;
const packages = new Map();
let currentPackage;
const decoder = new TextDecoder();
const string = (pointer, length) => {
  const bytes = new Uint8Array(exports.memory.buffer);
  if (length === undefined) { let end = pointer; while (bytes[end]) end++; length = end - pointer; }
  return decoder.decode(bytes.subarray(pointer, pointer + length));
};
const errors = [];
({ instance: { exports } } = await WebAssembly.instantiate(fs.readFileSync(modulePath), { env: {
  namespace(pointer, length) { packages.set(string(pointer, length), currentPackage); },
  resolve(pointer, outBytes, outLength) {
    const pkg = packages.get(string(pointer));
    if (!pkg) return 0;
    const memory = new DataView(exports.memory.buffer);
    memory.setUint32(outBytes, pkg.pointer, true); memory.setUint32(outLength, pkg.length, true);
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
for (const file of fs.readdirSync(libs).filter(file => file.endsWith('.zpkg'))) {
  currentPackage = store(fs.readFileSync(path.join(libs, file)));
  assert.equal(exports.index_package(currentPackage.pointer, currentPackage.length), 0, file);
}
const code = store(fs.readFileSync(zbc));
assert.equal(exports.run(code.pointer, code.length), 0, errors.join('\n'));
if (withCompression === 'false') assert(errors.some(error => error.includes('compression component unavailable')), errors.join('\n'));
else assert.deepEqual(errors, []);
console.log(`relinked WASM: interpreter execution + compression ${withCompression === 'true' ? 'roundtrip' : 'omission'} passed`);
