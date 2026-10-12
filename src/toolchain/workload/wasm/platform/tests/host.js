// Test host page driver. Loaded by index.html (which playwright opens).
// Exposes `window.__test.runRn()` async functions implementing the
// platform-test-contract R1–R7 scenarios. Each returns a value that the
// playwright spec.ts file asserts on.
//
// Spec: 变更 add-wasm-tests/specs/wasm-tests/spec.md
//       变更 define-platform-test-contract/specs/platform-test-contract/spec.md

import init, { Z42VM, readNamespaces } from '../pkg-web/z42_wasm.js';
import { mapResolver, bundleStdlibBrowser } from '../js/stdlib-resolver.js';

// ── Bootstrap. ──────────────────────────────────────────────────────

await init();

const STDLIB_URL = new URL('../js/stdlib/', import.meta.url);
const FIX_URL    = (name) => new URL(`../js/fixtures/${name}.zbc`, import.meta.url);

async function fetchBytes(url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`fetch ${url} → ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
}

async function loadFixture(name) {
    return await fetchBytes(FIX_URL(name));
}

function makeCollector() {
    let buf = new Uint8Array(0);
    return {
        handler(bytes) {
            const next = new Uint8Array(buf.length + bytes.length);
            next.set(buf);
            next.set(bytes, buf.length);
            buf = next;
        },
        get text() {
            return new TextDecoder().decode(buf);
        },
    };
}

async function makeVMWithSink() {
    const resolver = await bundleStdlibBrowser(STDLIB_URL, readNamespaces);
    const collector = makeCollector();
    const vm = new Z42VM({
        zpkgResolver: resolver,
        stdoutHandler: collector.handler,
    });
    return { vm, collector };
}

async function runHelloAndCollect() {
    const { vm, collector } = await makeVMWithSink();
    const mod = vm.loadZbc(await loadFixture('hello'));
    const entry = vm.resolveEntry(mod, 'Hello.Main');
    vm.invoke(entry, []);
    vm.dispose();
    return collector.text;
}

// ── R1–R7. Each function returns a `{ ok, ... }` object or throws. ──

window.__test = {

    // R1: smoke
    async runR1() {
        return await runHelloAndCollect();
    },

    // R2: bad zbc → status 10
    async runR2() {
        const { vm } = await makeVMWithSink();
        const garbage = new Uint8Array([0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03]);
        try {
            vm.loadZbc(garbage);
            return { thrown: false };
        } catch (err) {
            return { thrown: true, status: err?.status, name: err?.name };
        } finally {
            vm.dispose();
        }
    },

    // R3: unknown entry → status 20
    async runR3() {
        const { vm } = await makeVMWithSink();
        try {
            const mod = vm.loadZbc(await loadFixture('hello'));
            vm.resolveEntry(mod, 'App.Ghost');
            return { thrown: false };
        } catch (err) {
            return {
                thrown: true,
                status: err?.status,
                messageContainsFqn: (err?.message ?? '').includes('App.Ghost'),
            };
        } finally {
            vm.dispose();
        }
    },

    // R4: wrong arg count → status 21
    async runR4() {
        const { vm } = await makeVMWithSink();
        try {
            const mod = vm.loadZbc(await loadFixture('hello'));
            const entry = vm.resolveEntry(mod, 'Hello.Main');
            // Hello.Main is void(), so passing 1 arg trips the arg-count check.
            vm.invoke(entry, [42]);
            return { thrown: false };
        } catch (err) {
            return { thrown: true, status: err?.status };
        } finally {
            vm.dispose();
        }
    },

    // R5: MapResolver only knows "Std.Phantom" — load/invoke must fail.
    async runR5() {
        const phantomOnly = mapResolver(new Map([
            ['Std.Phantom', new Uint8Array(0)],
        ]));
        const vm = new Z42VM({
            zpkgResolver: phantomOnly,
            stdoutHandler: () => { /* discard */ },
        });
        try {
            const mod = vm.loadZbc(await loadFixture('hello'));
            const entry = vm.resolveEntry(mod, 'Hello.Main');
            vm.invoke(entry, []);
            return { thrown: false };
        } catch (err) {
            return { thrown: true, status: err?.status };
        } finally {
            vm.dispose();
        }
    },

    // R6: lifecycle × 3 — each round must produce the smoke output.
    async runR6() {
        const outputs = [];
        for (let i = 0; i < 3; i++) {
            outputs.push(await runHelloAndCollect());
        }
        return outputs;
    },

    // R7: multi-line stdout preserves byte order.
    async runR7() {
        const { vm, collector } = await makeVMWithSink();
        const mod = vm.loadZbc(await loadFixture('multi_line'));
        const entry = vm.resolveEntry(mod, 'MultiLine.Main');
        vm.invoke(entry, []);
        vm.dispose();
        return collector.text;
    },
};

window.__ready = true;
