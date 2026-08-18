//! Z42Value ↔ z42 Value conversion (blittable subset only).
//!
//! Spec C2 only marshals primitives + raw pointers. String / Array borrow
//! via `pinned` blocks lands in C4; managed object handles go through
//! `Z42_VALUE_TAG_OBJECT` once script-defined classes can cross FFI in C5.

use std::ffi::CString;
use std::os::raw::c_void;

use anyhow::{anyhow, Result};
use z42_abi::Z42Value;

use crate::metadata::Value;
use crate::native::dispatch::{
    self, SigType, Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64, Z42_VALUE_TAG_I64,
    Z42_VALUE_TAG_NATIVEPTR, Z42_VALUE_TAG_NULL,
};

/// Marshal failure surfaced to the caller (`call_native`).
///
/// `InvalidMarshal` is **user-catchable** — the caller constructs a
/// `Std.InvalidMarshalException` instance carrying the embedded message and
/// throws it through the interp's value-based propagation channel.
/// `Internal` covers VM-side programmer mismatches (IR / signature drift)
/// that cannot happen after a successful type check.
#[derive(Debug)]
pub enum MarshalErr {
    InvalidMarshal(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for MarshalErr {
    fn from(e: anyhow::Error) -> Self { Self::Internal(e) }
}

impl std::fmt::Display for MarshalErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarshalErr::InvalidMarshal(m) => write!(f, "{m}"),
            MarshalErr::Internal(e)       => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for MarshalErr {}

/// Per-call scratch storage owning temporary buffers (currently
/// NUL-terminated `CString`s) whose raw pointers were handed to a native
/// function. The arena is created on the stack of one `CallNative`
/// dispatch and dropped after the call returns; spec C8 added it to make
/// `(Value::Str, SigType::CStr)` marshal possible without leaking memory
/// or relying on thread-local storage.
#[derive(Default)]
pub struct Arena {
    cstrings: Vec<CString>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a CString into the arena and return its raw pointer. The
    /// pointer remains valid until the arena is dropped.
    fn alloc_cstring(&mut self, cs: CString) -> *const std::os::raw::c_char {
        let ptr = cs.as_ptr();
        self.cstrings.push(cs);
        ptr
    }

    /// Total number of stored temporaries — exposed for tests.
    pub fn temps_len(&self) -> usize { self.cstrings.len() }
}

/// Convert a z42 [`Value`] into a [`Z42Value`] suitable for passing to a
/// native method whose argument type is `target`. Temporaries that need
/// to outlive the conversion (e.g. `CString` backing for `*const c_char`)
/// are stashed in `arena` and remain alive until the caller drops it.
///
/// `Err(MarshalErr::InvalidMarshal)` indicates a user-recoverable failure
/// (interior NUL etc.) that the caller surfaces as
/// `Std.InvalidMarshalException`. All other errors are `Internal`.
pub fn value_to_z42(v: &Value, target: &SigType, arena: &mut Arena) -> Result<Z42Value, MarshalErr> {
    match (v, target) {
        // Integer-family targets accept Value::I64 (any narrowing happens
        // at libffi cif level — only the low N bytes are read).
        (Value::I64(n), SigType::I8 | SigType::I16 | SigType::I32 | SigType::I64
                       | SigType::U8 | SigType::U16 | SigType::U32 | SigType::U64) => {
            Ok(dispatch::z42_i64(*n))
        }
        // bool ↔ bool
        (Value::Bool(b), SigType::Bool) => Ok(dispatch::z42_bool(*b)),
        // f64 / f32 ← Value::F64
        (Value::F64(x), SigType::F64) => Ok(dispatch::z42_f64(*x)),
        (Value::F64(x), SigType::F32) => Ok(dispatch::z42_f64(*x)),
        // Native pointer slots accept Value::I64 (caller stores ptr-as-i64
        // until C5 introduces a real Value::NativePtr wrapper).
        (Value::I64(n), SigType::Ptr | SigType::SelfRef) => {
            Ok(dispatch::z42_native_ptr(*n as usize as *mut c_void))
        }
        (Value::Null, SigType::Ptr | SigType::SelfRef | SigType::CStr) => {
            Ok(dispatch::z42_native_ptr(std::ptr::null_mut()))
        }
        // Spec C4 — PinnedView projects to either its raw pointer or its length.
        // make-value-copy: PinnedView is now a transient-arena handle whose payload
        // (ptr/len) needs the `VmContext` arena to resolve, which this ctx-less marshal
        // helper cannot reach. The C5 source generator always emits `FieldGet view,"ptr"`
        // / `FieldGet view,"len"` (resolved through the arena in `exec_object::field_get`)
        // before the call site and passes the resulting scalars here — so a raw view never
        // reaches this arm on the compiler-emitted path. The previous "hand-crafted IR may
        // pass the view directly" convenience is retired (extends Decision 4: ctx-less
        // consumers of arena payloads degrade). Surfaces a clear error, not silent UB.
        (Value::PinnedView { .. }, _) => Err(MarshalErr::Internal(anyhow!(
            "PinnedView must be projected via FieldGet ptr/len before a native call \
             (make-value-copy: the view payload lives in the per-context transient arena, \
             not reachable from the ctx-less marshal path)"
        ))),
        // Spec C8 — Value::Str → *const c_char (NUL-terminated). The
        // arena owns the CString for the call's duration. Interior NULs
        // surface as Std.InvalidMarshalException (2026-05-11 retire-z-codes;
        // formerly Z0908) — the C consumer cannot disambiguate them from
        // the terminator.
        (Value::Str(s), SigType::CStr | SigType::Ptr) => {
            let cs = CString::new(&**s).map_err(|_| MarshalErr::InvalidMarshal(format!(
                "cannot pass z42 string {:?} as `*const c_char`: contains interior NUL",
                truncate_for_msg(s)
            )))?;
            let ptr = arena.alloc_cstring(cs);
            Ok(dispatch::z42_native_ptr(ptr as *mut c_void))
        }
        (v, ty) => Err(MarshalErr::Internal(anyhow!(
            "marshal: cannot pass z42 {v:?} as native arg of type {ty:?} (C2 blittable subset only; pinned/object marshalling lands in C4/C5)"
        ))),
    }
}

fn truncate_for_msg(s: &str) -> String {
    if s.len() <= 32 { s.to_string() } else { format!("{}…", &s[..32]) }
}

/// Convert the [`Z42Value`] returned from a native method into a z42 [`Value`].
pub fn z42_to_value(z: &Z42Value, source: &SigType) -> Result<Value> {
    match (z.tag, source) {
        (Z42_VALUE_TAG_NULL, SigType::Void) => Ok(Value::Null),
        (Z42_VALUE_TAG_NULL, _) => Ok(Value::Null),
        (Z42_VALUE_TAG_I64, _) => Ok(Value::I64(z.payload as i64)),
        (Z42_VALUE_TAG_F64, _) => Ok(Value::F64(f64::from_bits(z.payload))),
        (Z42_VALUE_TAG_BOOL, _) => Ok(Value::Bool(z.payload != 0)),
        (Z42_VALUE_TAG_NATIVEPTR, _) => {
            // C2 stores native pointers as Value::I64; C5 will introduce
            // a typed wrapper.
            Ok(Value::I64(z.payload as i64))
        }
        (tag, _) => Err(anyhow!(
            "z42_to_value: unsupported tag {tag} in C2 (Str/Object marshalling lands in C4/C5)"
        )),
    }
}
