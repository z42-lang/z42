// Browser hello-world demo for `@z42/wasm`.
//
// Mirrors `demo/node/run.js`: load corelib via stdlib bundle resolver,
// invoke `Hello.Main` (from the shared `src/toolchain/workload/fixtures/hello.z42`
// fixture), capture stdout. The only difference is that the .zbc +
// stdlib zpkgs are loaded via `fetch()` instead of `fs`, and the host
// writes captured output into the page DOM instead of stdout.
//
// Serve the wasm/ directory over HTTP (file:// URLs cannot load .wasm)
// and open this demo's index.html. See docs/workflow/building/wasm.md.

import init, { Z42VM, readNamespaces } from '../../pkg-web/z42_wasm.js';
import { bundleStdlibBrowser } from '../../js/stdlib-resolver.js';

const ZBC_URL    = new URL('../../js/fixtures/hello.zbc', import.meta.url);
const STDLIB_URL = new URL('../../js/stdlib/',            import.meta.url);

const logEl    = document.getElementById('log');
const statusEl = document.getElementById('status');

function setStatus(text, ok) {
    statusEl.textContent = text;
    statusEl.className   = ok ? 'ok' : 'err';
}

function append(line) {
    logEl.textContent += line;
}

async function main() {
    // wasm-pack `--target web` requires an explicit init() to fetch + instantiate
    // z42_wasm_bg.wasm before any exported class is touched.
    await init();

    const resolver = await bundleStdlibBrowser(STDLIB_URL, readNamespaces);

    let captured = '';
    const stdoutHandler = (bytes) => {
        const text = new TextDecoder().decode(bytes);
        captured += text;
        append(`[host] ${text}`);
    };

    const vm = new Z42VM({ zpkgResolver: resolver, stdoutHandler });

    const zbcBytes = new Uint8Array(await (await fetch(ZBC_URL)).arrayBuffer());
    const module   = vm.loadZbc(zbcBytes);
    const entry    = vm.resolveEntry(module, 'Hello.Main');
    vm.invoke(entry);
    vm.dispose();

    const expected = 'hello, world\n';
    if (captured !== expected) {
        throw new Error(
            `expected stdout ${JSON.stringify(expected)}, got ${JSON.stringify(captured)}`);
    }
    setStatus('OK', true);
}

main().catch((err) => {
    append(`\n[host] demo failed: ${err.stack ?? err}\n`);
    setStatus('FAILED', false);
});
