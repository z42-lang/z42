/* z42_abi.h — C ABI for native type interop.
 *
 * Native libraries use these declarations to register types and methods with
 * the z42 VM. See z42_host.h for host entry calls and z42_native.h for the
 * module function and script callback protocols.
 *
 * Contract: docs/reference/src/embedding/native-interop.md.
 *
 * ABI evolution rules:
 *   - `abi_version` MUST stay at offset 0 across all versions.
 *   - New struct fields are appended only; layout never reordered.
 *   - All access is through z42_* functions, not direct struct manipulation.
 *   - Major version bumps are explicit semver-major breaks.
 */

#ifndef Z42_ABI_H
#define Z42_ABI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Version ─────────────────────────────────────────────────────────────── */

#define Z42_ABI_VERSION 1

/* ── Flags ───────────────────────────────────────────────────────────────── */

/* Z42TypeDescriptor.flags */
#define Z42_TYPE_FLAG_VALUE_TYPE  (1u << 0)
#define Z42_TYPE_FLAG_SEALED      (1u << 1)
#define Z42_TYPE_FLAG_ABSTRACT    (1u << 2)
#define Z42_TYPE_FLAG_TRACEABLE   (1u << 3)  /* opts in to cycle collection */

/* Z42MethodDesc.flags */
#define Z42_METHOD_FLAG_STATIC    (1u << 0)
#define Z42_METHOD_FLAG_VIRTUAL   (1u << 1)
#define Z42_METHOD_FLAG_OVERRIDE  (1u << 2)
#define Z42_METHOD_FLAG_CTOR      (1u << 3)

/* Z42FieldDesc.flags */
#define Z42_FIELD_FLAG_READONLY   (1u << 0)
#define Z42_FIELD_FLAG_INTERNAL   (1u << 1)

/* ── Opaque handles ──────────────────────────────────────────────────────── */

/* Stable handle to a registered type. NULL = invalid / not found. */
typedef struct Z42Type* Z42TypeRef;

/* Scalar value tags shared by native interop and the host API. */
#define Z42_VALUE_TAG_NULL 0u
#define Z42_VALUE_TAG_I64  1u
#define Z42_VALUE_TAG_F64  2u
#define Z42_VALUE_TAG_BOOL 3u

/* Tagged value crossing the ABI. Scalars use integer bits, double bits,
 * or 0/1 for bool as their payload.
 */
typedef struct Z42Value {
    uint32_t tag;
    uint32_t reserved;
    uint64_t payload;   /* int / float bits / pointer / TypeRef */
} Z42Value;

/* Argument list passed into ctor / methods. Caller-allocated. */
typedef struct Z42Args {
    size_t count;
    const Z42Value* items;
} Z42Args;

/* Error sentinel returned from z42_last_error(). Tag 0 = no error. */
typedef struct Z42Error {
    uint32_t code;          /* Z0905..Z0910 (semantics by C2..C5) */
    const char* message;    /* static string or VM-owned; do not free */
} Z42Error;

/* ── Method / field / trait descriptors ──────────────────────────────────── */

typedef struct Z42MethodDesc {
    const char* name;
    const char* signature;  /* e.g. "(&Self, i64) -> Self" — parsed by VM */
    void*       fn_ptr;     /* extern "C" shim, z42 calling convention */
    uint32_t    flags;      /* Z42_METHOD_FLAG_* */
    uint32_t    reserved;
} Z42MethodDesc;

typedef struct Z42FieldDesc {
    const char* name;
    const char* type_name;  /* e.g. "u32", "*const Self" */
    size_t      offset;
    uint32_t    flags;      /* Z42_FIELD_FLAG_* */
    uint32_t    reserved;
} Z42FieldDesc;

typedef struct Z42MethodImpl {
    const char* name;
    void*       fn_ptr;
} Z42MethodImpl;

typedef struct Z42TraitImpl {
    const char* trait_name;     /* e.g. "z42.core::Display" */
    size_t      method_count;
    const Z42MethodImpl* methods;
} Z42TraitImpl;

/* ── Type descriptor (versioned; first field is abi_version) ─────────────── */

typedef struct Z42TypeDescriptor_v1 {
    uint32_t  abi_version;          /* MUST equal Z42_ABI_VERSION */
    uint32_t  flags;                /* Z42_TYPE_FLAG_* */
    const char* module_name;
    const char* type_name;
    size_t    instance_size;
    size_t    instance_align;

    /* Lifecycle */
    void*   (*alloc)(void);
    void    (*ctor)(void* self, const Z42Args* args);
    void    (*dtor)(void* self);
    void    (*dealloc)(void* self);
    void    (*retain)(void* self);
    void    (*release)(void* self);

    /* Methods */
    size_t                method_count;
    const Z42MethodDesc*  methods;

    /* Fields (optional exposure) */
    size_t                field_count;
    const Z42FieldDesc*   fields;

    /* Trait implementations */
    size_t                trait_impl_count;
    const Z42TraitImpl*   trait_impls;
} Z42TypeDescriptor_v1;

/* ── VM-exposed API (implemented by z42_vm) ──────────────────────────────── */

/* Register a native type with the VM. Returns NULL on failure;
 * call z42_last_error() to get the reason. */
Z42TypeRef z42_register_type(const Z42TypeDescriptor_v1* desc);

/* Resolve a previously registered type. NULL if not found. */
Z42TypeRef z42_resolve_type(const char* module, const char* type_name);

/* Last error encountered on the calling thread. code=0 means no error. */
Z42Error z42_last_error(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* Z42_ABI_H */
