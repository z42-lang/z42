//! Centralized JIT register-file access — the single choke point for reading
//! and writing `frame.regs` slots through the prologue-cached `regs_base`
//! pointer.
//!
//! Before this module (jit-unbox-regalloc Phase 2.0) the
//! `regs_base + idx * VALUE_STRIDE` address arithmetic and the
//! `VALUE_STRIDE` / `PAYLOAD_OFFSET` / `TAG_*` constants were **open-coded and
//! redeclared** in ~15 emitters in `translate.rs` (`emit_i64_binop`, `_cmp`,
//! `_convert`, `_neg`, `_bit_not`, `emit_primitive_copy`, `emit_const_*`, the
//! inline array/field/brcond arms, …). There was no `load_reg` / `store_reg`
//! choke point.
//!
//! Consolidating them here (a) removes the duplication / drift risk on the
//! Value layout and (b) gives the scalar-unbox + register-residency passes
//! (jit-unbox-regalloc 2B/2C) a *single* place to intercept register reads and
//! writes with a machine-register cache — the cache will replace `load_*` with
//! "return the cached SSA value if resident" and `store_*` with "update the
//! cache + mark dirty", spilling to memory only at the boundaries the design
//! enumerates (block terminators, Category-B calls, safepoints, OSR entry).
//!
//! Layout (pinned by `metadata::types_tests` + a compile-time `assert!` in
//! `metadata::types`): `Value` is 16 B, align 8, u8 discriminant at offset 0,
//! 8 B payload at offset 8.

use crate::metadata::Value;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value as ClifValue};
use cranelift_frontend::FunctionBuilder;

/// Stride between adjacent `Value` slots in `frame.regs` (== `size_of::<Value>()`).
pub const VALUE_STRIDE: i64 = std::mem::size_of::<Value>() as i64; // PR-5: 16 B
/// Byte offset of the 8-byte payload within a `Value` slot.
pub const PAYLOAD_OFFSET: i32 = 8;
/// Byte offset of the u8 discriminant (tag) within a `Value` slot.
pub const TAG_OFFSET: i32 = 0;

// ── Value discriminant bytes ─────────────────────────────────────────────────
// Mirror `Value`'s `#[repr(C, u8)]` order (`metadata/types.rs`), pinned by
// `value_discriminants_pinned` in `metadata/types_tests.rs`. Only the tags the
// JIT materializes natively are named here; heap tags (Array/Object/…) never
// get a native store (their payloads need Arc handling → helper path).
pub const TAG_I64: u8 = 0;
pub const TAG_F64: u8 = 1;
pub const TAG_BOOL: u8 = 2;
pub const TAG_CHAR: u8 = 3;
#[allow(dead_code)]
pub const TAG_STR: u8 = 4;
pub const TAG_NULL: u8 = 5;

/// Address of `frame.regs[reg]` = `regs_base + reg * VALUE_STRIDE`.
#[inline]
pub fn reg_addr(b: &mut FunctionBuilder, regs_base: ClifValue, reg: u32) -> ClifValue {
    let off = b.ins().iconst(types::I64, (reg as i64) * VALUE_STRIDE);
    b.ins().iadd(regs_base, off)
}

/// Load the i64 payload of a slot (low 8 bytes at offset 8). Valid for any slot
/// whose payload is a scalar bit-pattern the caller intends to read as i64
/// (I64 / narrow ints stored as i64 / Bool / Char). For F64 use
/// [`load_payload`] with `types::F64`.
#[inline]
pub fn load_payload_i64(b: &mut FunctionBuilder, addr: ClifValue) -> ClifValue {
    b.ins().load(types::I64, MemFlags::trusted(), addr, PAYLOAD_OFFSET)
}

/// Load the payload of a slot as the given Cranelift type at offset 8.
#[inline]
pub fn load_payload(b: &mut FunctionBuilder, addr: ClifValue, ty: types::Type) -> ClifValue {
    b.ins().load(ty, MemFlags::trusted(), addr, PAYLOAD_OFFSET)
}

/// Load the u8 discriminant (tag) of a slot at offset 0.
#[inline]
pub fn load_tag(b: &mut FunctionBuilder, addr: ClifValue) -> ClifValue {
    b.ins().load(types::I8, MemFlags::trusted(), addr, TAG_OFFSET)
}

/// Store a full slot: already-materialized u8 `tag` at offset 0 + `payload` at
/// offset 8.
#[inline]
pub fn store_tagged(b: &mut FunctionBuilder, addr: ClifValue, tag: ClifValue, payload: ClifValue) {
    b.ins().store(MemFlags::trusted(), tag, addr, TAG_OFFSET);
    b.ins().store(MemFlags::trusted(), payload, addr, PAYLOAD_OFFSET);
}

/// Store a slot whose discriminant is the compile-time constant `tag_u8`, plus
/// the `payload` value. Materializes the tag constant then delegates to
/// [`store_tagged`].
#[inline]
pub fn store_const_tag(b: &mut FunctionBuilder, addr: ClifValue, tag_u8: u8, payload: ClifValue) {
    let tag = b.ins().iconst(types::I8, tag_u8 as i64);
    store_tagged(b, addr, tag, payload);
}

/// Store only the u8 discriminant `tag_u8` at offset 0, leaving the payload
/// slot untouched. For discriminant-only values like `Value::Null` (whose tag
/// alone defines the value; caller verified the prior slot is Drop-free).
#[inline]
pub fn store_tag_const(b: &mut FunctionBuilder, addr: ClifValue, tag_u8: u8) {
    let tag = b.ins().iconst(types::I8, tag_u8 as i64);
    b.ins().store(MemFlags::trusted(), tag, addr, TAG_OFFSET);
}

/// Store only the payload (offset 8). Caller guarantees the tag byte already
/// holds the correct discriminant (e.g. narrowing convert whose dst tag stays
/// `TAG_I64`, or an in-place payload rewrite).
#[inline]
#[allow(dead_code)]
pub fn store_payload(b: &mut FunctionBuilder, addr: ClifValue, payload: ClifValue) {
    b.ins().store(MemFlags::trusted(), payload, addr, PAYLOAD_OFFSET);
}
