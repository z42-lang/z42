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
use std::collections::BTreeMap;

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

// ─── Block-local integer-scalar cache (jit-unbox-regalloc Phase 2B) ──────────
//
// Within a straight-line run of native integer ops, keep each register's
// unboxed i64 payload in a Cranelift SSA value instead of round-tripping
// through `frame.regs` memory every op. This is the manual store-to-load /
// redundant-load elimination Cranelift can't do for us: at `opt_level=none`
// (the VM's setting) it does no alias analysis, and even at `speed` it can't
// forward across the `regs_base` raw-pointer traffic + opaque helper calls
// (measured: zero compute gain — see `lazy.rs`). We *can*, because we know
// distinct reg indices never alias and which ops are pure.
//
// **Scope (deliberately narrow for correctness)**: caches only the i64 payload
// of integer-typed regs (all `I8..U64` are physically `Value::I64`, tag
// `TAG_I64`). Bool/Char/F64/heap values are never cached — they fall through
// to direct memory access. So a cached entry's tag is *always* `TAG_I64` and
// spilling is a plain `store_const_tag(TAG_I64, payload)`.
//
// **Coherence invariant** (the whole correctness argument): at every point
// where anything *other than* a cache-participating integer op could read or
// write `frame.regs` — every Category-B helper/call, every block terminator,
// every safepoint, and the start of every z42 block — the caller must have
// `flush`ed (spill dirty + clear) so memory is authoritative. `translate.rs`
// enforces this by flushing before every non-participating instruction and
// before the terminator, and by clearing at each block start. Cached SSA
// values therefore never cross a Cranelift block boundary (helpers that split
// the block via the `check!` macro are non-participating → flushed first) and
// never go stale (a helper that writes a reg is preceded by a flush that
// emptied the cache).
//
// Iteration/spill order is by reg index (`BTreeMap`) so codegen is
// deterministic — distinct slots make spill order semantically irrelevant, but
// determinism keeps the JIT reproducible.

/// One cached register: its unboxed i64 payload SSA value + whether the cache
/// is newer than memory (needs a spill).
#[derive(Clone, Copy)]
struct CacheEntry {
    payload: ClifValue,
    dirty: bool,
}

/// Block-local integer-scalar register cache. See module note above.
#[derive(Default)]
pub struct RegCache {
    entries: BTreeMap<u32, CacheEntry>,
}

impl RegCache {
    #[inline]
    pub fn new() -> Self {
        RegCache { entries: BTreeMap::new() }
    }

    /// Read `frame.regs[reg]`'s i64 payload, using the cached SSA value if the
    /// reg is resident; otherwise load from memory and record a clean entry.
    /// Only valid for integer-typed regs (caller guarantees via `reg_types`).
    #[inline]
    pub fn load_i64(&mut self, b: &mut FunctionBuilder, regs_base: ClifValue, reg: u32) -> ClifValue {
        if let Some(e) = self.entries.get(&reg) {
            return e.payload;
        }
        let addr = reg_addr(b, regs_base, reg);
        let v = load_payload_i64(b, addr);
        self.entries.insert(reg, CacheEntry { payload: v, dirty: false });
        v
    }

    /// Record `frame.regs[reg] = Value::I64(payload)` in the cache without
    /// writing memory; the value is spilled at the next `flush`.
    #[inline]
    pub fn store_i64(&mut self, reg: u32, payload: ClifValue) {
        self.entries.insert(reg, CacheEntry { payload, dirty: true });
    }

    /// Drop any cached entry for `reg` (its memory slot has just been written
    /// directly, e.g. a cmp storing a Bool). No spill — the cache value is dead.
    #[inline]
    pub fn invalidate(&mut self, reg: u32) {
        self.entries.remove(&reg);
    }

    /// Drop the entire cache without spilling — memory is already authoritative.
    /// Currently `translate.rs` builds a fresh `RegCache` per z42 block instead
    /// of reusing one, so this is unused today; kept as part of the cache API for
    /// Phase 2C (which will reuse a cache across the loop-header region).
    #[inline]
    #[allow(dead_code)]
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Spill every dirty entry back to `frame.regs` (as `Value::I64`), then
    /// clear the cache. Call before any Category-B op / terminator / safepoint —
    /// anywhere memory must be authoritative. Deterministic order (reg index).
    #[inline]
    pub fn flush(&mut self, b: &mut FunctionBuilder, regs_base: ClifValue) {
        for (reg, e) in std::mem::take(&mut self.entries) {
            if e.dirty {
                let addr = reg_addr(b, regs_base, reg);
                store_const_tag(b, addr, TAG_I64, e.payload);
            }
        }
    }
}
