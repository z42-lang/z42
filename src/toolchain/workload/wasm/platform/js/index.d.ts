/**
 * `@z42/wasm` — WebAssembly facade for the z42 embedding API.
 *
 * Spec: docs/design/runtime/embedding.md §6.2 (Tier 3 WASM)
 *       变更 add-platform-wasm/
 *
 * This file re-exports the wasm-bindgen-generated types from `pkg-web` /
 * `pkg-nodejs` under stable, cross-target names so user code stays the
 * same regardless of the resolved target.
 */

export type Z42VMValue =
    | null
    | boolean
    | number
    | bigint;

/**
 * Resolve a namespace (e.g. "z42.core" / "Std.IO") to its zpkg bytes,
 * or `null` when this resolver doesn't know about the namespace —
 * the runtime then falls back to other configured resolvers.
 *
 * Two accepted shapes:
 *   - A plain function `(name) => Uint8Array | null`
 *   - An object with a `resolve(name): Uint8Array | null` method
 *
 * The bytes returned **must remain valid** at least until the call
 * returns; the wasm runtime copies them eagerly. After return the
 * host may release / reuse the buffer.
 */
export type ZpkgResolverFn = (namespace: string) => Uint8Array | null;
export interface ZpkgResolverObject {
    resolve(namespace: string): Uint8Array | null;
}
export type ZpkgResolver = ZpkgResolverFn | ZpkgResolverObject;

export interface Z42VMOptions {
    /** Resolves stdlib + user zpkgs. See `stdlib-resolver.js` for built-ins. */
    zpkgResolver?: ZpkgResolver;
    /** Receives `Console.WriteLine` output. Called once per write. */
    stdoutHandler?: (bytes: Uint8Array) => void;
    /** Receives `Console.Error.WriteLine` output. */
    stderrHandler?: (bytes: Uint8Array) => void;
}

export class Z42VM {
    constructor(options?: Z42VMOptions);

    /** Parse a `.zbc` byte buffer into a loaded module. */
    loadZbc(bytes: Uint8Array): Z42VMModule;

    /** Resolve an entry by fully qualified name (e.g. "App.Main"). */
    resolveEntry(module: Z42VMModule, fqn: string): Z42VMEntry;

    /** Synchronously invoke an entry; H1 marshal supports null + i64/f64/bool. */
    invoke(entry: Z42VMEntry, args?: Z42VMValue[]): Z42VMValue;

    /** Explicit teardown. After this, the VM instance is unusable. */
    dispose(): void;
}

export class Z42VMModule {
    private constructor();
    free(): void;
}

export class Z42VMEntry {
    private constructor();
    free(): void;
}

/** Thrown for any non-OK status from the runtime. */
export class Z42VMError extends Error {
    /** Numeric Z42HostStatus value (1..99). */
    readonly status: number;
}

/** Initialise the wasm module. Required once per page / process. */
export default function init(input?: BufferSource | URL): Promise<void>;

/**
 * Read the namespaces a zpkg provides (its NSPC section). The
 * `stdlib-resolver.js` helpers use this to map namespace → bytes from the
 * packages directly — no `index.json`.
 */
export function readNamespaces(bytes: Uint8Array): string[];
