/* z42_host.h — C ABI for embedding and driving the z42 VM.
 *
 * Initialize a host, load a program, resolve and invoke an entry, then shut
 * down. Available directly and through the z42.h SDK entry point.
 *
 * Contract: docs/reference/src/embedding/c-abi.md.
 *   - `abi_version` MUST stay at offset 0 across all versions.
 *   - Within a layout version, new fields are appended. Set struct_size to
 *     sizeof(Z42HostConfig); the runtime reads only fields covered by it.
 *
 * One session may be active per process. Each load returns an independent
 * program with its own VM state. Shutdown invalidates all session handles.
 */

#ifndef Z42_HOST_H
#define Z42_HOST_H

#include "z42_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ── Version ─────────────────────────────────────────────────────────────── */

#define Z42_HOST_ABI_VERSION 3

/* ── Opaque handles ──────────────────────────────────────────────────────── */

/* Handle to the process's host session. NULL = invalid. */
typedef struct Z42Host*   Z42HostRef;

/* Handle to a loaded .zbc or .zpkg program. NULL = invalid. */
typedef struct Z42Module* Z42ModuleRef;

/* Handle to a resolved entry (function / static method). NULL = invalid. */
typedef struct Z42Entry*  Z42EntryRef;

/* ── Execution mode ──────────────────────────────────────────────────────── */

typedef enum Z42ExecMode {
    Z42_EXEC_MODE_DEFAULT = 0,  /* runtime build default */
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

/*
 * Read program code, package dependencies and application configuration by
 * their exact logical names. The request and writer are valid for the
 * duration of the callback on the calling thread. Keep user_data alive
 * until host shutdown. Implementations must synchronize shared state.
 */
#define Z42_FILE_ZPKG           1
#define Z42_FILE_ZBC            2
#define Z42_FILE_RUNTIME_CONFIG 3
#define Z42_FILE_ZBC_DEPS       4
#define Z42_FILE_ERROR          (-1)
#define Z42_FILE_NOT_FOUND      0
#define Z42_FILE_FOUND          1

typedef struct Z42FileRequest {
    uint32_t struct_size;
    uint32_t kind;
    const char* file_name; /* NUL-terminated UTF-8 logical name */
} Z42FileRequest;

typedef struct Z42FileWriter Z42FileWriter;

/*
 * On success call z42_file_write once and return Z42_FILE_FOUND.
 * Return Z42_FILE_NOT_FOUND without writing to allow normal lookup.
 * A read or submission failure returns Z42_FILE_ERROR.
 */
typedef int32_t (*Z42FileResolverFn)(const Z42FileRequest* request,
                                    Z42FileWriter* writer, void* user_data);

/* ── Configuration ───────────────────────────────────────────────────────── */

typedef struct Z42HostConfig {
    uint32_t      abi_version;        /* MUST equal Z42_HOST_ABI_VERSION */
    uint32_t      struct_size;        /* MUST be sizeof(Z42HostConfig) */

    Z42ExecMode   exec_mode;
    Z42WriteSink  stdout_sink;        /* NULL = real stdout */
    Z42WriteSink  stderr_sink;        /* NULL = real stderr */
    void*         sink_user_data;

    /* NULL-terminated array of module search paths. NULL = in-memory only. */
    const char* const* search_paths;

    /* Optional exact-name resolver and its callback context. */
    Z42FileResolverFn file_resolver;
    void* file_resolver_user_data;
} Z42HostConfig;

/* Required configuration prefix. Callers normally advertise sizeof(Z42HostConfig). */
#define Z42_HOST_CONFIG_MIN_SIZE \
    (offsetof(Z42HostConfig, file_resolver))

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

    /* Resolution / invocation */
    Z42_HOST_ERR_ENTRY_NOT_FOUND = 20,
    Z42_HOST_ERR_ARG_MISMATCH    = 21,

    /* Execution */
    Z42_HOST_ERR_VM_EXCEPTION    = 30,  /* z42 throw escaped the entry */
    Z42_HOST_ERR_FATAL           = 31,  /* fatal VM error (stack overflow) — shut the VM down */

    /* Catch-all */
    Z42_HOST_ERR_INTERNAL        = 99
} Z42HostStatus;

/* The ABI uses 32-bit enum fields and return values. These declarations
 * fail compilation on targets or compiler settings with shorter enums. */
typedef char Z42ExecModeMustBe32Bits[(sizeof(Z42ExecMode) == sizeof(int32_t)) ? 1 : -1];
typedef char Z42HostStatusMustBe32Bits[(sizeof(Z42HostStatus) == sizeof(int32_t)) ? 1 : -1];

/* ── Lifecycle API ───────────────────────────────────────────────────────── */

/*
 * Initialize the singleton VM instance. Thread-safe: serialized internally.
 *
 * Preconditions:
 *   - cfg != NULL and cfg->abi_version == Z42_HOST_ABI_VERSION
 *   - cfg->struct_size is the caller's sizeof(Z42HostConfig), at least
 *     Z42_HOST_CONFIG_MIN_SIZE
 *   - The process has no live VM (otherwise returns ERR_ALREADY_INIT).
 *
 * On success, *out_host is set to a non-NULL handle.
 */
Z42HostStatus z42_host_initialize(const Z42HostConfig* cfg,
                                  Z42HostRef* out_host);

/*
 * Copy bytes during a file resolver callback. bytes is borrowed until
 * this function returns. A zero-length buffer may use a NULL pointer.
 */
Z42HostStatus z42_file_write(Z42FileWriter* writer,
                            const uint8_t* bytes, size_t length);

/*
 * Load program code in either Z42 format. file_name is optional. A
 * supplied name can locate the application's runtimeconfig sidecar.
 * The input pointers are borrowed for the duration of the call.
 */
Z42HostStatus z42_host_load_bytes(Z42HostRef host, const char* file_name,
                                  const uint8_t* bytes, size_t length,
                                  Z42ModuleRef* out_module);

/*
 * Resolve an entry by fully qualified name. FQN format:
 *   "namespace.Type::method"  (static method or member)
 *   "namespace::function"     (top-level function)
 *
 */
Z42HostStatus z42_host_resolve_entry(Z42HostRef host, Z42ModuleRef module,
                                     const char* fqn,
                                     Z42EntryRef* out_entry);

/*
 * Synchronously invoke an entry. `args` and `n` must match the entry's
 * signature. `out_result` may be NULL to discard the return value.
 *
 */
Z42HostStatus z42_host_invoke(Z42EntryRef entry,
                              const Z42Value* args, size_t n,
                              Z42Value* out_result);

/*
 * Replace stdout / stderr sink at runtime. NULL clears the callback.
 * user_data is passed to subsequent callbacks.
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
 * stale handles returns ERR_NOT_INIT, even after a subsequent
 * z42_host_initialize creates a replacement session.
 */
Z42HostStatus z42_host_shutdown(Z42HostRef host);

/*
 * One-shot: load an app (.zbc / .zpkg) at `app_path` and run its
 * entry, then tear down.
 *
 *   app_path  path to the application (required)
 *   entry     entry FQN override, or NULL for the app's baked-in entry
 *   libs_dir  stdlib `libs/` dir (holds z42.core.zpkg + deps), or NULL
 *   argc/argv program args forwarded to the app's GetCommandLineArgs()
 *
 * Does NOT use the Z42HostRef handle (it builds + tears down its own VM).
 * Returns a process-style exit code: 0 ok, 1 run error, 2 null app, 70 panic.
 */
int z42_host_run_app(const char* app_path, const char* entry,
                     const char* libs_dir, int argc, const char* const* argv);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* Z42_HOST_H */
