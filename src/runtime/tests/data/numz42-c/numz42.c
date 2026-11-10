/* numz42-c — minimal Tier 1 native interop PoC for spec C2.
 *
 * Defines a single z42 type `Counter` with two methods (`inc`, `get`) and
 * one alloc-helper (`__alloc__`). Static descriptor + register entry mirror
 * what a real native library would publish.
 *
 * Linked statically into the integration test binary so the test does not
 * need to chase shared-library symbol-export rules. The test calls
 * `numz42_register_static()` directly; the dlopen path remains testable
 * once a host wants to ship this lib as a separate `.dylib`.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* z42_abi.h ships with the runtime. CMake / cc-rs build script adds
 * `src/runtime/include` to the include path. */
#include "z42_abi.h"
#include "z42_native.h"

size_t z42_native_layout(size_t index) {
    const size_t fields[] = {
        sizeof(Z42NativeText), sizeof(Z42NativeValue), sizeof(Z42NativeParameters),
        sizeof(Z42NativeCall), sizeof(Z42NativeFunction), sizeof(Z42NativeModule), sizeof(Z42NativeHostApi),
        offsetof(Z42NativeValue, bits), offsetof(Z42NativeValue, data),
        offsetof(Z42NativeModule, name), offsetof(Z42NativeModule, create_state),
        offsetof(Z42NativeFunction, invoke), offsetof(Z42NativeHostApi, argument)
    };
    return index < sizeof(fields) / sizeof(fields[0]) ? fields[index] : 0;
}

/* ── Counter type ───────────────────────────────────────────────────── */

typedef struct Counter {
    uint32_t rc;
    int64_t  value;
} Counter;

static void* counter_alloc(void) {
    Counter* c = (Counter*)malloc(sizeof(Counter));
    if (c) { c->rc = 1; c->value = 0; }
    return (void*)c;
}

static void counter_ctor(void* self, const Z42Args* args) {
    (void)args;
    Counter* c = (Counter*)self;
    c->rc = 1;
    c->value = 0;
}

static void counter_dtor(void* self) {
    (void)self;  /* no-op for POD payload */
}

static void counter_dealloc(void* self) {
    free(self);
}

static void counter_retain(void* self) {
    Counter* c = (Counter*)self;
    c->rc++;
}

static void counter_release(void* self) {
    Counter* c = (Counter*)self;
    if (--c->rc == 0) {
        counter_dtor(c);
        counter_dealloc(c);
    }
}

/* ── Methods exposed to z42 ─────────────────────────────────────────── */

/* Used by the integration test in lieu of a real ctor path (C5 wires
 * proper ctor / new(...) dispatch). */
static void* counter_alloc_method(void) {
    return counter_alloc();
}

static int64_t counter_inc(Counter* self) {
    return ++self->value;
}

static int64_t counter_get(const Counter* self) {
    return self->value;
}

/* Spec C8 — exercises Str→CStr marshal via the standard libc strlen.
 * Lives on Counter (rather than a dedicated type) so the test infra can
 * keep a single registered type; the Counter receiver itself is unused. */
static int64_t counter_strlen(const char* s) {
    return (int64_t)strlen(s);
}

/* Spec C10 — verifies Array<u8> pin: caller hands a (ptr, len) pair from
 * a pinned z42 byte[] and we just echo back the length we observed,
 * after touching every byte to force a real read. Returning ptr[0] would
 * also work but len is trivial to assert from the test side. */
static int64_t counter_buflen(const uint8_t* buf, uint64_t len) {
    /* Touch the buffer (so optimisers can't elide) — sum mod 256. */
    volatile uint64_t acc = 0;
    for (uint64_t i = 0; i < len; ++i) acc += buf[i];
    (void)acc;
    return (int64_t)len;
}

/* ── Static descriptor ──────────────────────────────────────────────── */

static const Z42MethodDesc COUNTER_METHODS[] = {
    {
        .name = "__alloc__",
        .signature = "() -> *mut Self",
        .fn_ptr = (void*)counter_alloc_method,
        .flags = Z42_METHOD_FLAG_STATIC,
        .reserved = 0,
    },
    {
        .name = "inc",
        .signature = "(*mut Self) -> i64",
        .fn_ptr = (void*)counter_inc,
        .flags = Z42_METHOD_FLAG_VIRTUAL,
        .reserved = 0,
    },
    {
        .name = "get",
        .signature = "(*const Self) -> i64",
        .fn_ptr = (void*)counter_get,
        .flags = Z42_METHOD_FLAG_VIRTUAL,
        .reserved = 0,
    },
    {
        .name = "strlen",
        .signature = "(*const u8) -> i64",
        .fn_ptr = (void*)counter_strlen,
        .flags = Z42_METHOD_FLAG_STATIC,
        .reserved = 0,
    },
    {
        .name = "buflen",
        .signature = "(*const u8, u64) -> i64",
        .fn_ptr = (void*)counter_buflen,
        .flags = Z42_METHOD_FLAG_STATIC,
        .reserved = 0,
    },
};

static const Z42TypeDescriptor_v1 COUNTER_DESC = {
    .abi_version    = Z42_ABI_VERSION,
    .flags          = Z42_TYPE_FLAG_SEALED,
    .module_name    = "numz42",
    .type_name      = "Counter",
    .instance_size  = sizeof(Counter),
    .instance_align = _Alignof(Counter),
    .alloc          = counter_alloc,
    .ctor           = counter_ctor,
    .dtor           = counter_dtor,
    .dealloc        = counter_dealloc,
    .retain         = counter_retain,
    .release        = counter_release,
    .method_count = sizeof(COUNTER_METHODS) / sizeof(COUNTER_METHODS[0]),
    .methods      = COUNTER_METHODS,
    .field_count = 0, .fields = NULL,
    .trait_impl_count = 0, .trait_impls = NULL,
};

/* ── Registration entry point ───────────────────────────────────────── */

/* Standard convention used by `loader::guess_register_symbol` when this
 * library is dlopened from a separate .dylib. */
void numz42_c_register(void) {
    z42_register_type(&COUNTER_DESC);
}

/* Stable alias the static-link integration test calls directly. */
void numz42_register_static(void) {
    numz42_c_register();
}
