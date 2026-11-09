//! z42 Tier 1 Native Interop ABI — Rust mirror of `include/z42_abi.h`.
//!
//! This crate is the canonical Rust definition of the stable C ABI types and
//! function declarations. It is `no_std` so it can be embedded in any context.
//!
//! Status: C1 scaffold. Function bodies are exposed by `z42_vm` and currently
//! return [`Z42Error`] with code `Z0905+` indicating "not implemented" — see
//! spec `design-interop-interfaces` and follow-up specs C2..C5.
//!
//! # Layout invariants
//!
//! - Every struct mirrors the C header with `#[repr(C)]` exact-layout ABI.
//! - [`Z42TypeDescriptor_v1`] keeps `abi_version` at offset 0 across versions.
//! - Field order is **frozen**; new fields append only on a major version bump.

#![no_std]

use core::ffi::{c_char, c_void};
pub mod native;

/// Current ABI major version. Mirrors `Z42_ABI_VERSION` in C header.
pub const Z42_ABI_VERSION: u32 = 1;

// ── Type flags ──────────────────────────────────────────────────────────────

pub const Z42_TYPE_FLAG_VALUE_TYPE: u32 = 1 << 0;
pub const Z42_TYPE_FLAG_SEALED: u32 = 1 << 1;
pub const Z42_TYPE_FLAG_ABSTRACT: u32 = 1 << 2;
pub const Z42_TYPE_FLAG_TRACEABLE: u32 = 1 << 3;

// ── Method flags ────────────────────────────────────────────────────────────

pub const Z42_METHOD_FLAG_STATIC: u32 = 1 << 0;
pub const Z42_METHOD_FLAG_VIRTUAL: u32 = 1 << 1;
pub const Z42_METHOD_FLAG_OVERRIDE: u32 = 1 << 2;
pub const Z42_METHOD_FLAG_CTOR: u32 = 1 << 3;

// ── Field flags ─────────────────────────────────────────────────────────────

pub const Z42_FIELD_FLAG_READONLY: u32 = 1 << 0;
pub const Z42_FIELD_FLAG_INTERNAL: u32 = 1 << 1;

// ── Opaque handles ──────────────────────────────────────────────────────────

/// Opaque registered-type handle.
///
/// Returned by [`z42_register_type`] / [`z42_resolve_type`]. `None` represents
/// the C-side `NULL` (invalid / not found).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Z42TypeRef(pub *mut Z42TypeOpaque);

/// Forward-declared opaque struct corresponding to `struct Z42Type` in C.
#[repr(C)]
pub struct Z42TypeOpaque {
    _private: [u8; 0],
}

/// Tagged value crossing the ABI boundary. Internal layout of `payload` is
/// stabilised by C2; for C1 it is treated as opaque bits.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Z42Value {
    pub tag: u32,
    pub reserved: u32,
    pub payload: u64,
}

/// Argument list passed into ctor / methods. Caller-allocated.
#[repr(C)]
pub struct Z42Args {
    pub count: usize,
    pub items: *const Z42Value,
}

/// Last-error sentinel. `code == 0` means no error.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Z42Error {
    pub code: u32,
    pub message: *const c_char,
}

// ── Method / field / trait descriptors ──────────────────────────────────────

/// Native lifecycle hook signatures (Rust-side aliases for C function pointers).
pub type Z42AllocFn = unsafe extern "C" fn() -> *mut c_void;
pub type Z42CtorFn = unsafe extern "C" fn(this: *mut c_void, args: *const Z42Args);
pub type Z42DtorFn = unsafe extern "C" fn(this: *mut c_void);
pub type Z42DeallocFn = unsafe extern "C" fn(this: *mut c_void);
pub type Z42RetainFn = unsafe extern "C" fn(this: *mut c_void);
pub type Z42ReleaseFn = unsafe extern "C" fn(this: *mut c_void);

#[repr(C)]
pub struct Z42MethodDesc {
    pub name: *const c_char,
    pub signature: *const c_char,
    pub fn_ptr: *mut c_void,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct Z42FieldDesc {
    pub name: *const c_char,
    pub type_name: *const c_char,
    pub offset: usize,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct Z42MethodImpl {
    pub name: *const c_char,
    pub fn_ptr: *mut c_void,
}

#[repr(C)]
pub struct Z42TraitImpl {
    pub trait_name: *const c_char,
    pub method_count: usize,
    pub methods: *const Z42MethodImpl,
}

// ── Type descriptor v1 (frozen layout) ──────────────────────────────────────

/// Native type descriptor. **Field order is frozen for ABI v1.**
///
/// `abi_version` MUST be the first field (offset 0) and MUST equal
/// [`Z42_ABI_VERSION`] when registering.
#[repr(C)]
pub struct Z42TypeDescriptor_v1 {
    pub abi_version: u32,
    pub flags: u32,
    pub module_name: *const c_char,
    pub type_name: *const c_char,
    pub instance_size: usize,
    pub instance_align: usize,

    pub alloc: Option<Z42AllocFn>,
    pub ctor: Option<Z42CtorFn>,
    pub dtor: Option<Z42DtorFn>,
    pub dealloc: Option<Z42DeallocFn>,
    pub retain: Option<Z42RetainFn>,
    pub release: Option<Z42ReleaseFn>,

    pub method_count: usize,
    pub methods: *const Z42MethodDesc,

    pub field_count: usize,
    pub fields: *const Z42FieldDesc,

    pub trait_impl_count: usize,
    pub trait_impls: *const Z42TraitImpl,
}

// ── Z42Value tag values (frozen) ───────────────────────────────────────────
//
// `Z42Value.tag` discriminator. Once shipped these values are part of the
// stable ABI; new entries append only.

pub const Z42_VALUE_TAG_NULL:        u32 = 0;
pub const Z42_VALUE_TAG_I64:         u32 = 1;
pub const Z42_VALUE_TAG_F64:         u32 = 2;
pub const Z42_VALUE_TAG_BOOL:        u32 = 3;
pub const Z42_VALUE_TAG_STR:         u32 = 4;
pub const Z42_VALUE_TAG_OBJECT:      u32 = 5;
pub const Z42_VALUE_TAG_TYPEREF:     u32 = 6;
pub const Z42_VALUE_TAG_NATIVEPTR:   u32 = 7;
/// Spec C4 — `Value::PinnedView` (z42 String/Array borrowed for FFI).
pub const Z42_VALUE_TAG_PINNED_VIEW: u32 = 8;

// ── Sync promises ─────────────────────────────────────────────────────────
//
// These descriptor types contain raw pointers (`*const c_char`,
// `*const Z42MethodDesc`, etc.) so they're `!Sync` by default. In every
// supported usage (including the C3 derive-macro expansion) the targets
// of those pointers live in `'static` storage — string literals,
// compile-time-built static arrays, `extern "C" fn` items, etc. We
// therefore promise `Sync` at the crate level so descriptors can be
// declared `static`.
//
// SAFETY: holders of these structs must guarantee every pointer field
// references `'static` immutable data. Violating this contract will be
// caught by the registry path (`registry::RegisteredType::from_descriptor`)
// when CStrings cannot be read or signatures fail to parse.

unsafe impl Sync for Z42TypeDescriptor_v1 {}
unsafe impl Sync for Z42MethodDesc {}
unsafe impl Sync for Z42FieldDesc {}
unsafe impl Sync for Z42MethodImpl {}
unsafe impl Sync for Z42TraitImpl {}

// ── VM-exposed API (resolved at link time against z42_vm) ───────────────────

unsafe extern "C" {
    pub fn z42_register_type(desc: *const Z42TypeDescriptor_v1) -> Z42TypeRef;

    pub fn z42_resolve_type(
        module: *const c_char,
        type_name: *const c_char,
    ) -> Z42TypeRef;

    pub fn z42_invoke(
        ty: Z42TypeRef,
        method: *const c_char,
        args: *const Z42Value,
        arg_count: usize,
    ) -> Z42Value;

    pub fn z42_invoke_method(
        receiver: Z42Value,
        method: *const c_char,
        args: *const Z42Value,
        arg_count: usize,
    ) -> Z42Value;

    pub fn z42_last_error() -> Z42Error;
}
