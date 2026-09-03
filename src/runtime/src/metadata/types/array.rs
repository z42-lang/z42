//! ArrayObj / ArrayBacking + 构造与 backing 分配。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// Primitive and heap value types that the VM operates on at runtime.
///
/// Integer types are unified as I64 (all integer arithmetic is 64-bit internally).
/// The compiler emits ConstI32/ConstI64 which the VM widens to I64.
/// Floating-point is unified as F64 (double precision).
///
/// `Array` / `Object` 用 [`GcRef<T>`] 作为不透明堆引用句柄。Phase 3a backing
/// 是 `Rc<RefCell<T>>`（行为等价历史 `Rc<RefCell<...>>` 直构）；Phase 3b 切到
/// 自定义堆 + mark-sweep 时，本 enum 与所有 callsite 保持不变。
///
/// `Value::Str` remains a primitive for performance; member access on strings
/// is handled via virtual field dispatch in the interpreter.
///
/// 2026-04-29 remove-dead-value-map: 删除了 `Value::Map` variant —— 自从
/// 2026-04-26 extern-audit-wave0 把 `Std.Collections.Dictionary` 改为纯 z42
/// 脚本类（基于 `T[]`），Map variant 已无创建路径，作为 dead variant 一并清理。
/// review.md C2 P1 step 0 (2026-05-28): `#[repr(C, u8)]` locks the
/// discriminant + payload memory layout so the JIT can emit raw
/// `load`/`store` Cranelift instructions against register slots
/// without going through `extern "C"` helpers. Layout invariants:
///   * offset 0 — u8 discriminant (explicit assignments below)
///   * offset 8 — payload (aligned to max-payload alignment = 8)
///   * total size — 24 B (max payload = `Str(Arc<str>)` at 16 B)
/// Niche optimisation on `Option<Value>` is lost vs natural enum
/// layout, but `Value` is never stored as `Option<Value>` on hot
/// paths — `Frame::ret: Option<Value>` is the sole site and is
/// touched once per function return. Layout is pinned by
/// `value_layout_tests.rs`; drift fails CI before bad JIT code emits.
/// add-reflection-array-element-type (2026-06-11): the heap payload behind
/// `Value::Array`. Carries the element type's FQ name (written by `ArrayNew` /
/// `ArrayNewLit` from the compile-time-known element type) so reflection is
/// non-erased — `arr.GetType().GetElementType()` returns the real element type.
/// Derefs to the element `Vec<Value>` (plus `Index`/`IndexMut`) so every
/// existing array operation (len / index / iterate / push) works unchanged.
/// unify-gc-heap PR-3: `ArrayObj` is a fixed-length array header. Its element
/// storage lives in the **single GC variable-length heap** (`region_var`),
/// referenced by a [`VarGcRef`] inside [`ArrayBacking`] — no external `Vec`
/// (the CLR/JVM single-heap model). The header itself lives in `region_array`
/// (`Mutex`-guarded); the backing block is uniquely owned by this header and
/// accessed only under its `borrow`/`borrow_mut` lock, so element reads/writes
/// against the raw block payload are race-free (D13).
///
/// `ArrayObj` is intentionally **not `Clone`** (a derived shallow clone would
/// alias the backing block, breaking value-semantic array copies) — use
/// [`ArrayObj::deep_copy`] for a heap-aware independent copy.
#[derive(Debug)]
pub struct ArrayObj {
    /// Element type FQ name (e.g. "int" / "geometry.Point"). Empty = unknown
    /// (Rust-synthesized arrays like reflection result sets; user arrays from
    /// `ArrayNew` always carry it).
    pub element_type: Arc<str>,
    /// packed-primitive-arrays: element storage. **Step 1a** introduces this
    /// enum with only `Boxed` (behaviour-identical refactor). **Step 1b** adds
    /// packed primitive backings (Bytes/Chars/I32/I64/F64/Bool) — the C#
    /// value-type-array model (inline packed, no per-element boxing, GC skips).
    pub backing: ArrayBacking,
}

/// Array element storage — the C# value-type-vs-reference array distinction.
/// unify-gc-heap PR-3: every variant's element buffer is now a GC variable-length
/// block (`VarGcRef` into `region_var`) instead of an external `Vec` — the single
/// GC heap. Blocks are fixed-size (z42 arrays don't grow) and non-moving; the
/// variant tag discriminates boxing semantics + block layout:
/// - `Boxed`  → `BlockType::ArrayValue` block of `len` `Value`s (each a traced edge).
/// - packed (`Bool`/`Bytes`/`I32`/`I64`/`Chars`/`F64`) → `BlockType::ArrayPrim`
///   block of `len` packed `T`s (POD leaf — GC skips, no per-element boxing).
/// - `StructBytes` → **two** blocks: `bytes` (`ArrayStruct`, POD packed struct
///   bytes) + `refs` (`ArrayValue`, the reference side-table, traced).
/// box/unbox happens only at the ArrayGet/ArraySet boundary because interp
/// registers are `Value` (the JIT reads/writes packed blocks unboxed).
#[derive(Debug)]
pub enum ArrayBacking {
    Boxed { block: VarGcRef, len: usize },
    Bool  { block: VarGcRef, len: usize },
    Bytes { block: VarGcRef, len: usize },  // byte / sbyte（窄整型并入；box 语义按 element_type）
    I32   { block: VarGcRef, len: usize },  // int / uint / short / ushort
    I64   { block: VarGcRef, len: usize },  // long / ulong
    Chars { block: VarGcRef, len: usize },  // char（scalar，与 String.ToCharArray 对齐）
    F64   { block: VarGcRef, len: usize },  // double / float
    /// add-struct-heap-inline (P3b, D1-a): a **value-struct array** `Point[]` — the
    /// C# inline `struct[]` model. `len` elements' bytes are packed back-to-back in
    /// the `bytes` block (`len * elem_size`); reference leaves live in the parallel
    /// `refs` block (`len * layout.ref_count()`, element `i`'s refs at
    /// `[i*rc, (i+1)*rc)`). `layout` = the element struct type's byte+reference layout
    /// (shared `Arc` — type metadata, not per-instance data, stays out of GC).
    /// Element access goes through a `Value::StructRefHeap` handle (route α), not
    /// `get_boxed`/`set_boxed` (those have no array `GcRef` to build a handle from).
    StructBytes {
        elem_size: usize,
        len: usize,
        bytes: VarGcRef,
        refs: VarGcRef,
        layout: std::sync::Arc<StructTypeLayout>,
    },
    /// add-escape-analysis-stack-alloc / unify-gc-heap PR-3: an **escape-analysis
    /// stack array** — a non-escaping array whose storage lives in the per-frame
    /// stack arena (`ctx.stack_arena`), **not** the GC heap. Boxed `Value`s inline
    /// in an arena-owned `Vec` (mirrors `StackObject`'s off-GC `Box` backing +
    /// `StackClosure`'s arena env — escape-analysis products deliberately bypass GC).
    /// Its elements are scanned directly as GC roots by the stack-arena root scanner,
    /// so no GC block / `mark_backing` is needed. Only the stack-alloc construction
    /// path (`ArrayObj::stack_typed`) produces this; heap arrays never carry it.
    StackVec(Vec<Value>),
}

impl ArrayObj {
    // ── block payload accessors (unify-gc-heap PR-3) ────────────────────────
    // Reinterpret a backing block's inline payload as a `&[T]` / `&mut [T]`.
    //
    // SAFETY (shared): the caller holds the `ArrayObj` under a `borrow()`
    // (shared region lock), so the block is alive and no writer aliases it; the
    // block stores exactly `len` `T`s (allocated `len*size_of::<T>()` bytes,
    // 8-aligned payload, `align_of::<T>() <= 8`). The returned slice's lifetime is
    // tied to the accessor's `&self`, and the block outlives the header (freed
    // only when the header is swept), so the borrow is sound.
    #[inline]
    pub(super) unsafe fn slice_of<T>(block: &VarGcRef, len: usize) -> &[T] {
        Self::debug_block_bounds::<T>(block, len, "slice_of");
        // SAFETY: see method contract; payload derived from the raw header ptr (D8).
        unsafe { std::slice::from_raw_parts(block.payload_as_ptr::<T>(), len) }
    }

    /// unify-gc-heap PR-3 safety guard: verify a `len`-element `T` view fits inside the
    /// backing block (alive + `len*size_of::<T>() <= payload_size`). Turns a would-be
    /// out-of-bounds / use-after-free block read into a clear panic instead of a raw SIGSEGV.
    #[inline]
    pub(super) fn debug_block_bounds<T>(block: &VarGcRef, len: usize, ctx: &str) {
        // Debug-only (per-access; off in release to keep the array hot path lean).
        debug_assert!(block.is_live(),
            "unify-gc-heap PR-3: {ctx}<{}> on a stale/tombstoned block (len={len})", std::any::type_name::<T>());
        debug_assert!(
            len.checked_mul(std::mem::size_of::<T>()).is_some_and(|need| need <= block.payload_size()),
            "unify-gc-heap PR-3: {ctx}<{}> OOB — len={len} elems > block payload {}", std::any::type_name::<T>(), block.payload_size());
    }
    // SAFETY (exclusive): additionally the caller holds `borrow_mut()` (exclusive
    // region lock) so this `&mut [T]` uniquely aliases the block payload.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub(super) unsafe fn slice_of_mut<T>(block: &VarGcRef, len: usize) -> &mut [T] {
        Self::debug_block_bounds::<T>(block, len, "slice_of_mut");
        // SAFETY: see method contract; exclusive access + payload from raw header ptr (D8).
        unsafe { std::slice::from_raw_parts_mut(block.payload_as_ptr::<T>(), len) }
    }

    /// Allocate a `Value` block (`BlockType::ArrayValue`) and **move** `elems` into
    /// it. Returns the handle + element count. The block payload is zero-initialized
    /// by the allocator (`I64(0)` — a POD `Value`); `ptr::write` overwrites each slot
    /// without dropping, so every slot ends up an initialized moved `Value`.
    pub(super) fn alloc_boxed(heap: &dyn MagrGC, elems: Vec<Value>) -> (VarGcRef, usize) {
        let len = elems.len();
        let block = heap.alloc_var_block(len * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `len` Values; write each moved value into its slot.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in elems.into_iter().enumerate() {
            // SAFETY: `base[i]` is one of `len` slots; `write` moves without dropping (POD zero).
            unsafe { base.add(i).write(v); }
        }
        (block, len)
    }

    /// Allocate a `Value` block and **clone** `src` into it (deep-copy / null-fill path).
    pub(super) fn alloc_values_clone(heap: &dyn MagrGC, src: &[Value]) -> VarGcRef {
        let block = heap.alloc_var_block(src.len() * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `src.len()` Values.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in src.iter().enumerate() {
            // SAFETY: slot `i` in a `src.len()`-slot block; `write` moves the clone in.
            unsafe { base.add(i).write(v.clone()); }
        }
        block
    }

    /// Allocate a `Value` block of `n` slots all initialized to `Null` (struct[]
    /// reference side-table default — the allocator's zero-init is `I64(0)`, not `Null`).
    pub(super) fn alloc_values_null(heap: &dyn MagrGC, n: usize) -> VarGcRef {
        let block = heap.alloc_var_block(n * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `n` Values.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for i in 0..n {
            // SAFETY: slot `i` of `n`; write Null over the POD zero without dropping.
            unsafe { base.add(i).write(Value::Null); }
        }
        block
    }

    /// Allocate a packed POD block (`BlockType::ArrayPrim`) and copy `data` into it.
    pub(super) fn alloc_packed<T: Copy>(heap: &dyn MagrGC, data: &[T]) -> VarGcRef {
        let block = heap.alloc_var_block(std::mem::size_of_val(data), BlockType::ArrayPrim);
        if !data.is_empty() {
            debug_assert!(std::mem::size_of_val(data) <= block.payload_size(),
                "unify-gc-heap PR-3: alloc_packed OOB write — {} bytes > block payload {}", std::mem::size_of_val(data), block.payload_size());
            // SAFETY: block payload sized `size_of_val(data)`, 8-aligned ≥ align_of::<T>();
            // src/dst are distinct, non-overlapping regions of `data.len()` `T`s.
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), block.payload_as_ptr::<T>(), data.len()); }
        }
        block
    }

    /// Untyped array (element type unknown) — for Rust-synthesized arrays.
    #[inline]
    pub fn new(heap: &dyn MagrGC, elems: Vec<Value>) -> Self {
        let (block, len) = Self::alloc_boxed(heap, elems);
        Self { element_type: Arc::from(""), backing: ArrayBacking::Boxed { block, len } }
    }
    /// Array with a known element type (from `ArrayNew` / `ArrayNewLit`).
    /// **Step 1b-ii**: primitive element types get a packed value-type backing
    /// (C# model); everything else stays `Boxed`. Unknown/FQN element types fall
    /// back to `Boxed` (safe — no packing, correct behaviour).
    #[inline]
    pub fn typed(heap: &dyn MagrGC, element_type: &str, elems: Vec<Value>) -> Self {
        Self { element_type: Arc::from(element_type), backing: Self::pack_backing(heap, element_type, elems) }
    }

    /// add-escape-analysis-stack-alloc / unify-gc-heap PR-3: build a **stack array**
    /// (escape-analysis non-escaping array) whose elements live in a plain arena-owned
    /// `Vec` — **no GC allocation** (the whole point of stack-alloc). Stored in
    /// `ctx.stack_arena`; its `Value` elements are scanned as GC roots. Unlike
    /// [`Self::typed`], this needs no heap and never packs (short-lived frame-local
    /// storage; boxed `Value`s keep the interp read/write path uniform).
    #[inline]
    pub fn stack_typed(element_type: &str, elems: Vec<Value>) -> Self {
        Self { element_type: Arc::from(element_type), backing: ArrayBacking::StackVec(elems) }
    }

    /// FFI return fast-path (packed-primitive-arrays Step 3): build a `byte[]`
    /// straight from an owned `Vec<u8>` — no per-byte `Value::I64` boxing, no
    /// re-pack scan. The mirror of `as_bytes()` on the ingest side. This is the
    /// "简化 extern call" return path: native call → `&[u8]` → `byte[]` directly.
    pub fn from_bytes(heap: &dyn MagrGC, bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let block = Self::alloc_packed(heap, &bytes);
        Self { element_type: Arc::from("byte"), backing: ArrayBacking::Bytes { block, len } }
    }

    /// add-struct-array-codegen (P3b follow-up): build a value-struct array `Point[len]`
    /// with `StructBytes` backing — `len` elements packed back-to-back (`len*elem_size`
    /// bytes, zero-initialized = default struct) + a `Null`-filled reference side-table
    /// (`len*ref_count`). `layout` = the element struct type's byte+reference layout.
    /// Element access goes through a `Value::StructRefHeap` handle (see `array_get`).
    pub fn struct_backed(heap: &dyn MagrGC, element_type: &str, len: usize, layout: std::sync::Arc<StructTypeLayout>) -> Self {
        let elem_size = layout.size;
        let ref_count = layout.ref_count();
        // bytes: POD packed struct bytes, zero-init = default struct (allocator zeroes).
        let bytes = heap.alloc_var_block(len * elem_size, BlockType::ArrayStruct);
        // refs: reference side-table, Null-initialized (zero-init would be I64(0), wrong default).
        let refs = Self::alloc_values_null(heap, len * ref_count);
        Self {
            element_type: Arc::from(element_type),
            backing: ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout },
        }
    }

    /// Select a packed value-type backing for a primitive `element_type`,
    /// unboxing `elems` into it. Conservative + sign-safe: only widths that
    /// round-trip losslessly through `get_boxed`/`set_boxed` are packed.
    pub(super) fn pack_backing(heap: &dyn MagrGC, element_type: &str, elems: Vec<Value>) -> ArrayBacking {
        match element_type {
            // byte[] → contiguous u8: the FFI zero-copy + 24× memory win.
            "byte" | "u8" => {
                let v: Vec<u8> = elems.iter().map(|x| if let Value::I64(n) = x { *n as u8 } else { 0 }).collect();
                ArrayBacking::Bytes { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            "char" => {
                let v: Vec<char> = elems.iter().map(|x| if let Value::Char(c) = x { *c } else { '\0' }).collect();
                ArrayBacking::Chars { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            "bool" => {
                let v: Vec<bool> = elems.iter().map(|x| matches!(x, Value::Bool(true))).collect();
                ArrayBacking::Bool { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            // fits i32 signed range (i8/i16/i32 and u16 ≤ 65535).
            "sbyte" | "i8" | "short" | "i16" | "int" | "i32" | "ushort" | "u16" => {
                let v: Vec<i32> = elems.iter().map(|x| if let Value::I64(n) = x { *n as i32 } else { 0 }).collect();
                ArrayBacking::I32 { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            // 64-bit (uint/u32 fit i64; u64 keeps existing i64-store semantics).
            "long" | "i64" | "uint" | "u32" | "ulong" | "u64" | "isize" | "usize" => {
                let v: Vec<i64> = elems.iter().map(|x| if let Value::I64(n) = x { *n } else { 0 }).collect();
                ArrayBacking::I64 { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            "double" | "float" | "f32" | "f64" => {
                let v: Vec<f64> = elems.iter().map(|x| if let Value::F64(f) = x { *f } else { 0.0 }).collect();
                ArrayBacking::F64 { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            // object / string / nested arrays / structs / unknown FQN → reference array.
            _ => {
                let (block, len) = Self::alloc_boxed(heap, elems);
                ArrayBacking::Boxed { block, len }
            }
        }
    }
}
