//! libffi-backed dispatch: signature parsing, cif construction, and the
//! actual call into a registered native method.
//!
//! The [`SigType`] enum is the C2 blittable subset; richer types (`&[T]`,
//! `String`, `Result<T,E>`) come in C4/C5 along with the `pinned` block
//! and source generator.

use std::os::raw::c_void;

use anyhow::{anyhow, Result};
use libffi::middle::{Arg, Cif, CodePtr, Type};
use z42_abi::Z42Value;

/// One operand type in a Tier 1 native method signature. C2 supports the
/// blittable subset only; the source generator (C5) is responsible for
/// rejecting non-blittable signatures during compile-time validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigType {
    Void,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Bool,
    /// Any of `*const T`, `*mut T`. The element type is irrelevant at the
    /// ABI layer (all pointers have the same wire size).
    Ptr,
    /// Distinguished `Self` / `&Self` / `&mut Self` placeholder. Wire form
    /// is identical to [`SigType::Ptr`]; kept separate so marshal /
    /// diagnostics can render `Self` instead of `*mut void`.
    SelfRef,
    /// Borrowed C string (NUL-terminated). Wire form is a pointer.
    CStr,
}

impl SigType {
    pub fn ffi_type(&self) -> Type {
        match self {
            SigType::Void => Type::void(),
            SigType::I8   => Type::i8(),
            SigType::I16  => Type::i16(),
            SigType::I32  => Type::i32(),
            SigType::I64  => Type::i64(),
            SigType::U8   => Type::u8(),
            SigType::U16  => Type::u16(),
            SigType::U32  => Type::u32(),
            SigType::U64  => Type::u64(),
            SigType::F32  => Type::f32(),
            SigType::F64  => Type::f64(),
            SigType::Bool => Type::u8(), // C bool ABI = 1 byte
            SigType::Ptr | SigType::SelfRef | SigType::CStr => Type::pointer(),
        }
    }

    pub fn size_bytes(&self) -> usize {
        match self {
            SigType::Void => 0,
            SigType::I8 | SigType::U8 | SigType::Bool => 1,
            SigType::I16 | SigType::U16 => 2,
            SigType::I32 | SigType::U32 | SigType::F32 => 4,
            SigType::I64 | SigType::U64 | SigType::F64 => 8,
            SigType::Ptr | SigType::SelfRef | SigType::CStr => std::mem::size_of::<*mut c_void>(),
        }
    }
}

/// Parses a method signature string of the form `"(T1, T2, ...) -> R"`.
///
/// Recognised types: `i8..i64`, `u8..u64`, `f32`, `f64`, `bool`, `usize`,
/// `isize`, `CStr`, `*const T`, `*mut T`, `Self`, `&Self`, `&mut Self`,
/// `void` (or `()` for the return type).
pub fn parse_signature(s: &str) -> Result<(Vec<SigType>, SigType)> {
    let s = s.trim();
    let arrow = s.find("->").ok_or_else(|| anyhow!("signature missing '->': {s:?}"))?;
    let lhs = s[..arrow].trim();
    let rhs = s[arrow + 2..].trim();

    if !lhs.starts_with('(') || !lhs.ends_with(')') {
        anyhow::bail!("signature parameter list not parenthesized: {s:?}");
    }
    let inner = &lhs[1..lhs.len() - 1];

    let params: Vec<SigType> = if inner.trim().is_empty() {
        Vec::new()
    } else {
        inner
            .split(',')
            .map(|t| parse_type(t.trim()))
            .collect::<Result<_>>()?
    };
    let ret = parse_type(rhs)?;
    Ok((params, ret))
}

fn parse_type(s: &str) -> Result<SigType> {
    let s = s.trim();
    match s {
        "()" | "void" => Ok(SigType::Void),
        "i8" => Ok(SigType::I8),
        "i16" => Ok(SigType::I16),
        "i32" => Ok(SigType::I32),
        "i64" => Ok(SigType::I64),
        "u8" => Ok(SigType::U8),
        "u16" => Ok(SigType::U16),
        "u32" => Ok(SigType::U32),
        "u64" => Ok(SigType::U64),
        "f32" => Ok(SigType::F32),
        "f64" => Ok(SigType::F64),
        "bool" => Ok(SigType::Bool),
        "usize" => Ok(SigType::U64),
        "isize" => Ok(SigType::I64),
        "CStr" => Ok(SigType::CStr),
        s if s == "Self" || s == "&Self" || s == "&mut Self"
            || s == "*const Self" || s == "*mut Self" => Ok(SigType::SelfRef),
        s if s.starts_with("*const ") || s.starts_with("*mut ") => Ok(SigType::Ptr),
        _ => Err(anyhow!(
            "unsupported signature type {s:?} in C2 (high-level types arrive with `pinned` blocks in spec C4)"
        )),
    }
}

/// Build a libffi `Cif` from parsed parameter / return types.
pub fn build_cif(params: &[SigType], ret: &SigType) -> Cif {
    let arg_types: Vec<Type> = params.iter().map(SigType::ffi_type).collect();
    Cif::new(arg_types, ret.ffi_type())
}

/// Per-call argument scratch buffer. Each slot is wide enough to hold the
/// largest blittable type; libffi reads only the bytes corresponding to the
/// registered cif parameter type, so writing the value into the low N bytes
/// (little-endian) is sufficient.
#[repr(C, align(8))]
struct Slot {
    bytes: [u8; 16],
}

fn write_slot(z: &Z42Value, ty: &SigType, slot: &mut Slot) {
    // payload is u64; for narrower types the low bytes carry the value
    // (correct on little-endian platforms — z42 has no big-endian target).
    let p = z.payload.to_le_bytes();
    let n = ty.size_bytes();
    if n > 0 {
        slot.bytes[..n].copy_from_slice(&p[..n.min(8)]);
        if n > 8 {
            // safety net for future >8-byte primitives; currently unused
            slot.bytes[8..n].fill(0);
        }
    }
}

/// Invoke a native method via its prepared `cif`.
///
/// # Safety
/// Caller guarantees:
/// - `cif` was built from `params` / `ret` that match `fn_ptr`'s real signature
/// - `args.len() == params.len()`
/// - `fn_ptr` is a valid function pointer alive for the call duration
/// - z42 thread-local state (`CURRENT_VM`, panic guards) is set if `fn_ptr`
///   may re-enter z42 via `z42_invoke`
pub unsafe fn call(
    cif: &Cif,
    fn_ptr: *mut c_void,
    args: &[Z42Value],
    params: &[SigType],
    ret: &SigType,
) -> Result<Z42Value> {
    if args.len() != params.len() {
        anyhow::bail!(
            "argument count mismatch: cif expects {}, got {}",
            params.len(),
            args.len()
        );
    }

    let mut slots: Vec<Slot> = (0..args.len()).map(|_| Slot { bytes: [0; 16] }).collect();
    for (i, (z, p)) in args.iter().zip(params.iter()).enumerate() {
        write_slot(z, p, &mut slots[i]);
    }
    let arg_refs: Vec<Arg> = slots.iter().map(|s| Arg::new(&s.bytes)).collect();
    let code = CodePtr(fn_ptr);

    Ok(match ret {
        SigType::Void => {
            let _: () = unsafe { cif.call(code, &arg_refs) };
            z42_null()
        }
        SigType::I8 => {
            let r: i8 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::I16 => {
            let r: i16 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::I32 => {
            let r: i32 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::I64 => {
            let r: i64 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r)
        }
        SigType::U8 => {
            let r: u8 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::U16 => {
            let r: u16 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::U32 => {
            let r: u32 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::U64 => {
            let r: u64 = unsafe { cif.call(code, &arg_refs) };
            z42_i64(r as i64)
        }
        SigType::F32 => {
            let r: f32 = unsafe { cif.call(code, &arg_refs) };
            z42_f64(r as f64)
        }
        SigType::F64 => {
            let r: f64 = unsafe { cif.call(code, &arg_refs) };
            z42_f64(r)
        }
        SigType::Bool => {
            let r: u8 = unsafe { cif.call(code, &arg_refs) };
            z42_bool(r != 0)
        }
        SigType::Ptr | SigType::SelfRef | SigType::CStr => {
            let r: *mut c_void = unsafe { cif.call(code, &arg_refs) };
            z42_native_ptr(r)
        }
    })
}

// ── Z42Value constructors (kept here so dispatch + marshal share form) ─────

pub(crate) fn z42_null() -> Z42Value {
    Z42Value { tag: Z42_VALUE_TAG_NULL, reserved: 0, payload: 0 }
}
pub(crate) fn z42_i64(v: i64) -> Z42Value {
    Z42Value { tag: Z42_VALUE_TAG_I64, reserved: 0, payload: v as u64 }
}
pub(crate) fn z42_f64(v: f64) -> Z42Value {
    Z42Value { tag: Z42_VALUE_TAG_F64, reserved: 0, payload: v.to_bits() }
}
pub(crate) fn z42_bool(v: bool) -> Z42Value {
    Z42Value { tag: Z42_VALUE_TAG_BOOL, reserved: 0, payload: if v { 1 } else { 0 } }
}
pub(crate) fn z42_native_ptr(v: *mut c_void) -> Z42Value {
    Z42Value { tag: Z42_VALUE_TAG_NATIVEPTR, reserved: 0, payload: v as usize as u64 }
}

// ── Frozen Z42Value tag values ──────────────────────────────────────────
//
// Source of truth lives in `z42_abi`; re-exported here for the historical
// callers in this module. New consumers should import from `z42_abi`
// directly.

pub use z42_abi::{
    Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64, Z42_VALUE_TAG_I64, Z42_VALUE_TAG_NATIVEPTR,
    Z42_VALUE_TAG_NULL, Z42_VALUE_TAG_OBJECT, Z42_VALUE_TAG_PINNED_VIEW, Z42_VALUE_TAG_STR,
    Z42_VALUE_TAG_TYPEREF,
};
