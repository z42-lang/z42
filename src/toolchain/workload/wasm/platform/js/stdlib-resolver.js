// Built-in stdlib resolvers for `@z42/wasm`. Each returns a ZpkgResolver
// function `(namespace) => Uint8Array | null` — the shape Z42VM's
// `zpkgResolver` option expects (the load hook is unchanged).
//
// Resolution model — read NSPC, no index file:
//   The runtime asks the resolver for a namespace's bytes. These helpers
//   build the namespace → bytes map by reading each zpkg's NSPC section
//   (via the wasm `readNamespaces` export), so a single zpkg that ships
//   several namespaces (e.g. z42.core.zpkg → z42.core + Std + Std.Exceptions)
//   maps all of them. There is no hand-maintained `index.json`.
//
//   Node.js: enumerate the package's `stdlib/` dir via readdir.
//   Browser: HTTP can't list a directory, so fetch a build-generated
//     `files.json` (a plain list of zpkg filenames, derived — never
//     hand-maintained) and fetch each.
//
//   `readNamespaces` is the wasm export — import it from the resolved
//   target (`pkg-web` / `pkg-nodejs` / `@z42/wasm`) and pass it in.
//
// Spec: 变更 drop-index-json-self-describing/
//       变更 add-wasm-tests/  (wasm side)

/**
 * Build a synchronous resolver from a pre-populated namespace → bytes map.
 * Also the building block for active injection: a host that already holds
 * zpkg bytes (web playground / REPL) builds the map and passes the resolver.
 */
export function mapResolver(byNamespace) {
    return function resolve(namespace) {
        return byNamespace.get(namespace) ?? null;
    };
}

/**
 * Read each zpkg's NSPC and assemble a namespace → bytes map. `zpkgs` is
 * an iterable of `Uint8Array`; `readNamespaces` is the wasm export
 * `(bytes) => string[]`. First-wins on duplicate namespaces.
 */
export function buildNamespaceMap(zpkgs, readNamespaces) {
    const byNamespace = new Map();
    for (const bytes of zpkgs) {
        let namespaces;
        try {
            namespaces = readNamespaces(bytes);
        } catch {
            continue; // not a parseable zpkg — skip
        }
        for (const ns of namespaces) {
            if (!byNamespace.has(ns)) byNamespace.set(ns, bytes);
        }
    }
    return byNamespace;
}

/**
 * Node-only: read every `*.zpkg` from the package's `stdlib/` dir, map
 * namespaces via NSPC, return a resolver.
 *
 *     import { readNamespaces } from '@z42/wasm';
 *     const vm = new Z42VM({ zpkgResolver: await bundleStdlibNode(readNamespaces) });
 */
export async function bundleStdlibNode(readNamespaces) {
    const fs = await import('node:fs');
    const path = await import('node:path');
    const url = await import('node:url');

    const here = path.dirname(url.fileURLToPath(import.meta.url));
    const stdlibDir = path.join(here, 'stdlib');
    if (!fs.existsSync(stdlibDir)) return mapResolver(new Map());

    const zpkgs = fs.readdirSync(stdlibDir)
        .filter((f) => f.endsWith('.zpkg'))
        .sort()
        .map((f) => new Uint8Array(fs.readFileSync(path.join(stdlibDir, f))));
    return mapResolver(buildNamespaceMap(zpkgs, readNamespaces));
}

/**
 * Browser-only: fetch every zpkg listed in the build-generated
 * `files.json` under `baseUrl`, map namespaces via NSPC, return a resolver.
 *
 *     import { readNamespaces } from '@z42/wasm';
 *     const resolver = await bundleStdlibBrowser(
 *         new URL('./stdlib/', import.meta.url), readNamespaces);
 *     const vm = new Z42VM({ zpkgResolver: resolver });
 */
export async function bundleStdlibBrowser(baseUrl, readNamespaces) {
    if (!(baseUrl instanceof URL)) {
        baseUrl = new URL(baseUrl, location.href);
    }
    const filenames = (await fetchFileList(baseUrl)).slice().sort();
    const zpkgs = (await Promise.all(filenames.map(async (filename) => {
        const res = await fetch(new URL(filename, baseUrl));
        return res.ok ? new Uint8Array(await res.arrayBuffer()) : null;
    }))).filter((b) => b !== null);
    return mapResolver(buildNamespaceMap(zpkgs, readNamespaces));
}

// ── Helpers. ────────────────────────────────────────────────────────

async function fetchFileList(baseUrl) {
    try {
        const res = await fetch(new URL('files.json', baseUrl));
        if (res.ok) {
            const list = await res.json();
            if (Array.isArray(list)) return list;
        }
    } catch {
        /* fall through to empty list */
    }
    return [];
}
