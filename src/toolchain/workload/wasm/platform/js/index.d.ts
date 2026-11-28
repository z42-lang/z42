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

export type FileKind = 1 | 2 | 3 | 4; // ZPKG, ZBC, runtimeconfig, ZBC dependencies
export type FileResolverFn = (fileName: string, kind: FileKind) => Uint8Array | null;
export type FileResolver = FileResolverFn | { resolve: FileResolverFn };

export interface Z42VMOptions {
    /** Resolves stdlib + user zpkgs. See `stdlib-resolver.js` for built-ins. */
    fileResolver?: FileResolver;
    /** Receives `Console.WriteLine` output. Called once per write. */
    stdoutHandler?: (bytes: Uint8Array) => void;
    /** Receives `Console.Error.WriteLine` output. */
    stderrHandler?: (bytes: Uint8Array) => void;
}

export class Z42VM {
    constructor(options?: Z42VMOptions);

    /** Parse a `.zbc` byte buffer into a loaded module. */
    loadZbc(bytes: Uint8Array): Z42VMModule;
    loadBytes(bytes: Uint8Array, fileName?: string): Z42VMModule;

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
