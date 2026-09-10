/* z42_host.h — Tier 1 Embedding / Hosting ABI
 *
 * Stable C ABI for host applications to embed the z42 VM:
 *   initialize → load .zbc → resolve entry → invoke → shutdown.
 *
 * Sibling to z42_abi.h, which covers "native code registers types into z42".
 * This header covers the opposite direction: "host app drives the VM".
 *
 * Spec: docs/design/runtime/embedding.md (Tier 1 C ABI §4)
 *       docs/spec/archive/2026-05-10-add-embedding-api/
 *
 * Status: H1 scaffold — declarations + single-instance lifecycle.
 *         load_zbc / resolve_entry / invoke return ERR_INTERNAL
 *         (placeholder for H2 implementation).
 *
 * ABI evolution rules (mirror z42_abi.h):
 *   - `abi_version` MUST stay at offset 0 across all versions.
 *   - New struct fields are appended only; layout never reordered.
 *   - All access is through z42_host_* functions, not direct struct manipulation.
 *   - Major version bumps are explicit semver-major breaks.
 *
 * v0.1 lifecycle: single VM instance per process.
 *   z42_host_initialize() succeeds at most once at a time;
 *   shutdown returns the process to "uninitialized" so initialize may be
 *   called again. Multi-instance / ALC-like contexts are tracked in
 *   docs/design/runtime/embedding.md §12 Deferred.
 */

#ifndef Z42_HOST_H
#define Z42_HOST_H

#include "z42_abi.h"   /* Z42Value / Z42Args / Z42Error */

#ifdef __cplusplus
extern "C" {
#endif

/* ── Version ─────────────────────────────────────────────────────────────── */

#define Z42_HOST_ABI_VERSION 1

/* ── Opaque handles ──────────────────────────────────────────────────────── */

/* Handle to the (singleton, in v0.1) VM instance. NULL = invalid. */
typedef struct Z42Host*   Z42HostRef;

/* Handle to a loaded .zbc module. NULL = invalid. */
typedef struct Z42Module* Z42ModuleRef;

/* Handle to a resolved entry (function / static method). NULL = invalid. */
typedef struct Z42Entry*  Z42EntryRef;

/* ── Execution mode ──────────────────────────────────────────────────────── */

typedef enum Z42ExecMode {
    Z42_EXEC_MODE_DEFAULT = 0,  /* decided by .zbc metadata + compile-time features */
    Z42_EXEC_MODE_INTERP  = 1,
    Z42_EXEC_MODE_JIT     = 2,  /* feature=jit must be enabled */
    Z42_EXEC_MODE_AOT     = 3   /* feature=aot must be enabled */
} Z42ExecMode;

/* ── Output sink ─────────────────────────────────────────────────────────── */

/*
 * stdout / stderr write sink. `length` does not include a NUL terminator
 * and the buffer is NOT guaranteed to be NUL-terminated. The sink callback
 * MUST NOT retain `bytes` past the call.
 *
 * Threading: callback runs on whichever thread emitted the output. Hosts
 * are responsible for thread-safe handling of `user_data`.
 */
typedef void (*Z42WriteSink)(const char* bytes, size_t length, void* user_data);

/* ── zpkg resolver hook ──────────────────────────────────────────────────── */

/*
 * Resolve a namespace to its zpkg bytes. The runtime calls this during
 * z42_host_load_zbc whenever the user .zbc imports a namespace not yet
 * loaded; "z42.core" is always probed even when the user code has no
 * `using` statements.
 *
 * Contract:
 *   - On hit, write *out_bytes / *out_length and return non-zero.
 *     Bytes need only stay valid until this call returns — the runtime
 *     copies what it needs before returning to the host.
 *   - On miss, return 0. The runtime ignores *out_bytes / *out_length
 *     and falls back to search_paths (if set).
 *
 * The `int` return (instead of Z42HostStatus) is intentional: a resolver
 * has only hit/miss semantics, not a runtime error space.
 *
 * Spec: docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/
 */
typedef int (*Z42ZpkgResolverFn)(
    const char*     namespace_name,
    const uint8_t** out_bytes,
    size_t*         out_length,
    void*           user_data);

/* ── zpkg namespace introspection ────────────────────────────────────────── */

/*
 * Visitor for z42_zpkg_read_namespaces: called once per resolution key a
 * zpkg answers to — its package name (e.g. "z42.core") and each namespace
 * it declares in NSPC (e.g. "Std", "Std.IO"). `ns` is `len` UTF-8 bytes
 * (NOT NUL-terminated), valid only for the duration of the call — copy
 * before returning. Lets a resolver build its map from the package
 * itself, so no separate index file is needed.
 */
typedef void (*Z42NamespaceVisitor)(const char* ns, size_t len, void* user_data);

/* ── Configuration ───────────────────────────────────────────────────────── */

typedef struct Z42HostConfig {
    uint32_t      abi_version;        /* MUST equal Z42_HOST_ABI_VERSION */
    uint32_t      reserved;

    Z42ExecMode   exec_mode;
    size_t        heap_initial_bytes; /* 0 = VM default */
    size_t        heap_max_bytes;     /* 0 = unbounded */

    Z42WriteSink  stdout_sink;        /* NULL = real stdout */
    Z42WriteSink  stderr_sink;        /* NULL = real stderr */
    void*         sink_user_data;

    /* NULL-terminated array of module search paths. NULL = in-memory only. */
    const char* const* search_paths;

    /*
     * Optional zpkg resolver hook. When set, the runtime tries it FIRST
     * for every namespace lookup during load_zbc; falls back to
     * search_paths on miss. NULL = "no resolver; scan search_paths only".
     *
     * APPENDED to v1 layout (2026-05-11). ABI version stays at 1 since
     * adding trailing fields is forward-compatible (callers built against
     * older headers leave the field NULL when zero-initialising cfg).
     */
    Z42ZpkgResolverFn  zpkg_resolver;
    void*              zpkg_resolver_user_data;
} Z42HostConfig;

/* ── Status codes ────────────────────────────────────────────────────────── */

typedef enum Z42HostStatus {
    Z42_HOST_OK                  = 0,

    /* Lifecycle */
    Z42_HOST_ERR_ALREADY_INIT    = 1,
    Z42_HOST_ERR_NOT_INIT        = 2,
    Z42_HOST_ERR_BAD_CONFIG      = 3,   /* abi_version mismatch / null cfg / etc. */
    Z42_HOST_ERR_FEATURE_OFF     = 4,   /* requested mode disabled at compile time */

    /* Module loading */
    Z42_HOST_ERR_BAD_ZBC         = 10,  /* magic / checksum failure */
    Z42_HOST_ERR_VERIFICATION    = 11,  /* IR verification failure */

    /* Resolution / invocation */
    Z42_HOST_ERR_ENTRY_NOT_FOUND = 20,
    Z42_HOST_ERR_ARG_MISMATCH    = 21,

    /* Execution */
    Z42_HOST_ERR_VM_EXCEPTION    = 30,  /* z42 throw escaped the entry */

    /* Catch-all */
    Z42_HOST_ERR_INTERNAL        = 99
} Z42HostStatus;

/* ── Lifecycle API ───────────────────────────────────────────────────────── */

/*
 * Initialize the singleton VM instance. Thread-safe: serialized internally.
 *
 * Preconditions:
 *   - cfg != NULL and cfg->abi_version == Z42_HOST_ABI_VERSION
 *   - The process has no live VM (otherwise returns ERR_ALREADY_INIT).
 *
 * On success, *out_host is set to a non-NULL handle.
 */
Z42HostStatus z42_host_initialize(const Z42HostConfig* cfg,
                                  Z42HostRef* out_host);

/*
 * Load a .zbc module from a byte buffer. The bytes must remain valid
 * for the duration of the call; the VM copies what it needs.
 *
 * H1 status: returns ERR_INTERNAL with message "H2: load_zbc not yet
 * implemented". Real loading lands in H2.
 */
Z42HostStatus z42_host_load_zbc(Z42HostRef host,
                                const uint8_t* bytes, size_t length,
                                Z42ModuleRef* out_module);

/*
 * Read the keys a zpkg can be resolved by — its package name plus every
 * namespace it declares in NSPC — invoking `visit` once per key.
 * Stateless — needs no initialized VM. Lets a resolver build its map from
 * the package itself (no index file). Returns ERR_BAD_ZBC if `bytes`
 * isn't a parseable zpkg.
 *
 * Spec: docs/spec/archive/2026-06-06-drop-index-json-self-describing/
 */
Z42HostStatus z42_zpkg_read_namespaces(const uint8_t* bytes, size_t length,
                                       Z42NamespaceVisitor visit, void* user_data);

/*
 * Resolve an entry by fully qualified name. FQN format:
 *   "namespace.Type::method"  (static method or member)
 *   "namespace::function"     (top-level function)
 *
 * H1 status: returns ERR_INTERNAL ("H2: resolve_entry not yet implemented").
 */
Z42HostStatus z42_host_resolve_entry(Z42HostRef host, Z42ModuleRef module,
                                     const char* fqn,
                                     Z42EntryRef* out_entry);

/*
 * Synchronously invoke an entry. `args` and `n` must match the entry's
 * signature. `out_result` may be NULL to discard the return value.
 *
 * H1 status: returns ERR_INTERNAL ("H2: invoke not yet implemented").
 */
Z42HostStatus z42_host_invoke(Z42EntryRef entry,
                              const Z42Value* args, size_t n,
                              Z42Value* out_result);

/*
 * Replace stdout / stderr sink at runtime. NULL restores the configured
 * default. user_data is passed to subsequent callbacks.
 */
Z42HostStatus z42_host_set_stdout_sink(Z42HostRef host,
                                       Z42WriteSink sink, void* user_data);
Z42HostStatus z42_host_set_stderr_sink(Z42HostRef host,
                                       Z42WriteSink sink, void* user_data);

/*
 * Last error from the calling thread (TLS). On success Z42HostStatus
 * resets `code` to 0 and `message` to a stable empty string.
 */
Z42Error z42_host_last_error(Z42HostRef host);

/*
 * Tear down the singleton VM. After this returns OK, all Z42ModuleRef /
 * Z42EntryRef issued by this VM are invalidated; calling host APIs with
 * stale handles returns ERR_NOT_INIT. A subsequent z42_host_initialize
 * is allowed.
 */
Z42HostStatus z42_host_shutdown(Z42HostRef host);

/*
 * One-shot: load a self-contained app (.zbc / .zpkg) at `app_zpkg` and run its
 * entry, then tear down. This is the embedded-app-run path (add-embedded-app-run
 * / add-wasm-testhost G6) shared by every platform's test-host and by workload
 * apps — desktop C shell, iOS Swift, Android JNI all call this one symbol.
 *
 *   app_zpkg  path to the app artifact (required)
 *   entry     entry FQN override, or NULL for the app's baked-in entry
 *   libs_dir  stdlib `libs/` dir (holds z42.core.zpkg + deps), or NULL
 *   argc/argv program args forwarded to the app's GetCommandLineArgs()
 *
 * Does NOT use the Z42HostRef handle (it builds + tears down its own VM).
 * Returns a process-style exit code: 0 ok, 1 run error, 2 null app, 70 panic.
 */
int z42_host_run_app(const char* app_zpkg, const char* entry,
                     const char* libs_dir, int argc, const char* const* argv);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* Z42_HOST_H */
