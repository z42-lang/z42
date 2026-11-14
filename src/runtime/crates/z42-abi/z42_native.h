/* Native module registration ABI. Mirrors crates/z42-abi/src/module.rs.
 * Descriptor spans are immutable and retained with module code. Call spans are
 * borrowed only for a synchronous, same-thread invocation. No Rust containers,
 * managed object addresses or unwind may cross this boundary.
 */
#ifndef Z42_NATIVE_H
#define Z42_NATIVE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

#define Z42_NATIVE_MODULE_ABI_VERSION 1
#define Z42_NATIVE_OK 0
#define Z42_NATIVE_ERROR 1
#define Z42_NATIVE_THROWN 2
#define Z42_NATIVE_NULL 0
#define Z42_NATIVE_I64 1
#define Z42_NATIVE_F64 2
#define Z42_NATIVE_BOOL 3
#define Z42_NATIVE_STR 4
#define Z42_NATIVE_OPT_STR 5
#define Z42_NATIVE_BYTES 6
#define Z42_NATIVE_STR_ARRAY 7
#define Z42_NATIVE_VOID 8
#define Z42_NATIVE_PURE 1
#define Z42_NATIVE_BLOCKING 2
#define Z42_NATIVE_REENTERS 4
#define Z42_NATIVE_CAP_LOADING UINT64_C(1)
#define Z42_NATIVE_CAP_TERMINAL UINT64_C(2)
#define Z42_NATIVE_CAP_FILESYSTEM UINT64_C(4)
#define Z42_NATIVE_CAP_CONSOLE_INPUT UINT64_C(8)

typedef struct Z42NativeText { const uint8_t* data; size_t len; } Z42NativeText;
typedef struct Z42NativeValue {
    uint32_t kind;
    uint32_t reserved;
    uint64_t bits;
    const uint8_t* data;
    size_t len;
} Z42NativeValue;
typedef struct Z42NativeParameters { const uint32_t* data; size_t len; } Z42NativeParameters;
typedef struct Z42NativeHostApi Z42NativeHostApi;
typedef struct Z42NativeCall {
    const Z42NativeHostApi* host;
    void* context;
    void* state;
} Z42NativeCall;
typedef uint32_t (*Z42NativeInvoke)(Z42NativeCall*);
typedef struct Z42NativeFunction {
    Z42NativeText name;
    Z42NativeParameters parameters;
    uint32_t result;
    uint32_t flags;
    Z42NativeInvoke invoke;
} Z42NativeFunction;
typedef struct Z42NativeModule {
    uint32_t version;
    uint32_t size;
    Z42NativeText name;
    const Z42NativeFunction* functions;
    size_t function_count;
    void* (*create_state)(void);
    void (*destroy_state)(void*);
} Z42NativeModule;
typedef const Z42NativeModule* (*Z42NativeModuleEntry)(void);

struct Z42NativeHostApi {
    uint32_t version;
    uint32_t size;
    uint64_t capabilities;
    uint32_t (*argument)(Z42NativeCall*, size_t, Z42NativeValue*);
    uint32_t (*return_value)(Z42NativeCall*, const Z42NativeValue*);
    uint32_t (*raise)(Z42NativeCall*, Z42NativeText, Z42NativeText);
    uint32_t (*invoke_script)(Z42NativeCall*, Z42NativeText, const Z42NativeValue*, size_t, Z42NativeValue*);
    uint32_t (*resolve_symbol)(Z42NativeCall*, Z42NativeText, Z42NativeText, void**);
};

/* Export <logical_library_name>_module_v1 from a dynamic library, or invoke
 * the same entry directly from a statically linked host. Stateful modules
 * provide paired create/destroy callbacks; only their own code dereferences state.
 * Host return_value copies borrowed data before returning. STR_ARRAY data points
 * to len Z42NativeValues, each with kind STR. VOID and NULL remain distinct.
 */
#ifdef __cplusplus
}
#endif
#endif
