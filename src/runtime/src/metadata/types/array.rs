//! ArrayObj / ArrayBacking + 构造与 backing 分配。元素读写在 `array_elems`，视图 / GC / 拷贝在
//! `array_access`。
//!
//! 数组元素按分配时的元素类型分模式（object model R3）——堆上每个可变单元 ≤ 8 B、同宽原子读写：
//!
//! | 模式 | backing | 元素 | 读 / 写 |
//! |---|---|---|---|
//! | PRIM | `Bool` / `Bytes` / `I32` / `I64` / `Chars` / `F64` | 打包基元，原宽度 | 同宽 relaxed 原子 |
//! | REF | `Refs` | 8 B 自描述引用字（`ref_word`） | acquire 读 / release 写，标记期 swap 给 SATB |
//! | MIXED | `Cells` | 16 B 型参单元（`tparam_cell`：标签字 + 负载字） | 型参单元协议，无锁 |
//! | STRUCT | `StructBytes` | 值 struct blob，引用叶子是引用字（`struct_blob`） | 叶子各按其单元 |
//! | FROZEN | `Frozen` | 16 B `Value`，发布前写一次、之后只读 | 普通读 |
//! | 栈数组 | `StackVec` | 帧 arena 里的 `Value`（GC 根） | — |

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// The heap payload behind `Value::Array`: the element type (non-erased reflection —
/// `arr.GetType().GetElementType()`) and the element storage. The storage is a fixed-size,
/// non-moving block in the GC variable-length heap (`region_var`), uniquely owned by this header
/// (the CLR / JVM single-heap model).
///
/// `ArrayObj` is intentionally **not `Clone`** (a shallow clone would alias the backing block) —
/// use [`ArrayObj::deep_copy`] for a heap-aware independent copy.
#[derive(Debug)]
pub struct ArrayObj {
    /// Element type FQ name (e.g. "int" / "geometry.Point"). Empty = unknown (Rust-synthesized
    /// arrays like reflection result sets). An interned 8 B handle ([`ElemType`]).
    pub element_type: ElemType,
    /// Element storage; the variant is the array's mode (see the module docs).
    pub backing: ArrayBacking,
}

/// Array element storage — one GC variable-length block per array (none for a stack array).
#[derive(Debug)]
pub enum ArrayBacking {
    /// REF: `len` 8 B reference words (`BlockType::ArrayRef`) — `string[]`, `object[]`, class /
    /// interface / delegate / array element types. A value that does not fit 8 B (a raw
    /// primitive reaching `object[]` through erased generics) is boxed (`ref_word` kind 7).
    Refs  { block: VarGcRef, len: usize },
    /// MIXED: `len` 16 B type-parameter cells (`BlockType::ArrayCell`) — erased `T[]` and arrays
    /// whose element type is unknown. Element `i`'s tag word is at `16 i`, its payload word at
    /// `16 i + 8`; the cell protocol (`tparam_cell`) keeps a read from ever tearing. `zero` is the
    /// primitive kind a never-written cell (tag word `0`) reads as — the zero of `T` when `T` was a
    /// primitive at allocation (`new T[n]` with `T = int`), `0` = `null` — so the default fill
    /// writes nothing and unused capacity (a `List<int>`'s tail) is never touched.
    Cells { block: VarGcRef, len: usize, zero: u8 },
    /// FROZEN: `len` raw 16 B `Value`s (`BlockType::ArrayValue`), written **once** before the
    /// array is published and only read afterwards: closure environments and the one-element
    /// boxes behind `ref_word` kind 7 (which may hold a stack handle no cell can encode).
    Frozen { block: VarGcRef, len: usize },
    Bool  { block: VarGcRef, len: usize },
    Bytes { block: VarGcRef, len: usize },  // byte / sbyte（窄整型并入；box 语义按 element_type）
    I32   { block: VarGcRef, len: usize },  // int / uint / short / ushort
    I64   { block: VarGcRef, len: usize },  // long / ulong
    Chars { block: VarGcRef, len: usize },  // char（scalar，与 String.ToCharArray 对齐）
    F64   { block: VarGcRef, len: usize },  // double / float
    /// STRUCT: a value-struct array `Point[]` — the C# inline `struct[]` model. `len` blobs of
    /// `layout.size` bytes back-to-back in one `BlockType::ArrayStruct` block; each element is
    /// exactly the standalone blob layout, its reference leaves 8 B reference words
    /// (`struct_blob`). Element access goes through a `Value::StructRefHeap` handle.
    StructBytes {
        len: usize,
        block: VarGcRef,
        layout: std::sync::Arc<StructTypeLayout>,
    },
    /// An escape-analysis **stack array**: `Value`s in an arena-owned `Vec` (not the GC heap),
    /// scanned directly as GC roots by the stack-arena root scanner. Only
    /// [`ArrayObj::stack_typed`] produces this.
    StackVec(Vec<Value>),
}

/// Byte stride of a REF element.
pub const REF_STRIDE: usize = 8;
/// Byte stride of a MIXED element (tag word + payload word).
pub const CELL_STRIDE: usize = 16;

/// 一个 `Value` 与它要写进的**基元 backing** 类型不符时的信号（make-silent-fallbacks-signal）。
///
/// 此前 `set_boxed` 与 `pack_backing` 的每个「类型不符」回落都是**静默存 0**
/// （`0` / `'\0'` / `false` / `0.0`），debug 与 release 都不响 —— 而 `set_boxed` 同一个
/// `match` 里的两个 `StructBytes` 臂**是带 `debug_assert!(false)` 的**。
///
/// 为什么这比同族的「读不到就给 `Null`」更坏：`Null` 至少是个可疑值，会在下游某处炸出来；
/// `0` 是程序**完全无法与合法写入区分**的答案 —— 一个 `int[]` 里的 `0` 既可能是用户写的，
/// 也可能是这里吞掉的一次类型错误。
///
/// 取 `debug_assert!` 而不是 `bail!`，判据同 `__box_prim` 收到 `Null` 那条：
/// - golden 语料默认跑 debug VM ⇒ **CI 本身就是探测器**；
/// - 「类型不符」是编译器该在 `ArraySet` / `ArrayNewLit` 站点转换/拆箱掉的事，**不是用户的错**，
///   所以 release 放行、保持今天的行为（不拿用户崩溃换诊断能力）；
/// - 若若干版本一直不响，再按 `gc/refs.rs` 那条先例考虑提升为无条件 `assert!`。
///
/// ⚠️ **2026-09-27（fix-silent-array-elem-zero）划清了适用范围**：上面第二条
/// 「不是用户的错」**只对 IR 站点（`ArraySet` / `ArrayNewLit`）成立** —— 那里编译器确实该先
/// 转换。它**不适用于无类型 API**：`Std.Array.SetValue(Object value, int index)` 的形参声明
/// 就是 `Object`、`Array.CopyRange` 的两侧元素类型可不同，**没有任何编译器站点能转换它们**，
/// 值是用户给的。实测那两条路在 release 下静默把 `int[0]` 从 9 变成 **0**（连
/// `double[] <- 整数` 这种在 C# 合法的写法也一样），故它们已改走
/// [`ArrayObj::try_set_boxed`] / 前置逐元素校验，**两个 profile 都报错**。
/// ⇒ 本函数现在只服务 IR 那一类调用方。
#[inline]
pub(super) fn prim_value_mismatch(val: &Value, backing: &str, site: &str) {
    debug_assert!(false,
        "array {site}: {backing} backing got a non-matching Value ({val:?}) — stored a zero. \
         A silent zero is indistinguishable from a legitimate write.\n\
         → From an IR `ArraySet` / `ArrayNewLit`: the COMPILER should have converted/unboxed at \
         that site — that is the bug to fix (this is what the debug-only policy below is for).\n\
         → From an UNTYPED API (`Std.Array.SetValue(Object, int)`, or `Array.CopyRange` across \
         element types): the value came from user code and NO compiler site could have converted \
         it. Those two paths validate up front and raise instead (`try_set_boxed`, change \
         fix-silent-array-elem-zero) — reaching here from them means a validation path was \
         bypassed, so add the check there rather than relaxing this assert.");
}

impl ArrayObj {
    // ── block payload accessors ───────────────────────────────────────────────
    // SAFETY (shared): the block is alive while the caller holds the `ArrayObj`; it stores exactly
    // `len` `T`s (8-aligned payload, `align_of::<T>() <= 8`). The returned slice's lifetime is tied
    // to the borrow, and the block outlives the header. A plain slice: only for memory nobody
    // writes after publication — FROZEN blocks (written once before the array is published).
    #[inline]
    pub(super) unsafe fn slice_of<T>(block: &VarGcRef, len: usize) -> &[T] {
        Self::debug_block_bounds::<T>(block, len, "slice_of");
        // SAFETY: see method contract; payload derived from the raw header ptr (D8).
        unsafe { std::slice::from_raw_parts(block.payload_as_ptr::<T>(), len) }
    }

    /// Verify a `len`-element `T` view fits inside the backing block (alive +
    /// `len*size_of::<T>() <= payload_size`) — a clear panic instead of a raw SIGSEGV.
    #[inline]
    pub(super) fn debug_block_bounds<T>(block: &VarGcRef, len: usize, ctx: &str) {
        debug_assert!(block.is_live(),
            "{ctx}<{}> on a stale/tombstoned block (len={len})", std::any::type_name::<T>());
        debug_assert!(
            len.checked_mul(std::mem::size_of::<T>()).is_some_and(|need| need <= block.payload_size()),
            "{ctx}<{}> OOB — len={len} elems > block payload {}", std::any::type_name::<T>(), block.payload_size());
    }

    // SAFETY (exclusive): additionally the block is **fresh** — allocated by the caller and not yet
    // reachable from any published array — so this `&mut [T]` uniquely aliases the payload.
    // Construction only: a published array is shared without a lock (object model R4).
    #[inline]
    #[allow(clippy::mut_from_ref)]
    pub(super) unsafe fn slice_of_mut<T>(block: &VarGcRef, len: usize) -> &mut [T] {
        Self::debug_block_bounds::<T>(block, len, "slice_of_mut");
        // SAFETY: see method contract.
        unsafe { std::slice::from_raw_parts_mut(block.payload_as_ptr::<T>(), len) }
    }

    /// Atomic loads and stores over `bytes` bytes of a backing block.
    #[inline(always)]
    pub(super) fn cells_of(block: &VarGcRef, bytes: usize) -> Cells<'_> {
        Self::debug_block_bounds::<u8>(block, bytes, "cells_of");
        // SAFETY: a live, 8-aligned block payload of at least `bytes` bytes, owned by the array
        // the caller borrows; every concurrent access is an atomic cell access.
        unsafe { Cells::from_raw(block.payload_as_ptr::<u8>(), bytes) }
    }

    /// Allocate a FROZEN block and move `elems` into it.
    fn alloc_frozen(heap: &dyn MagrGC, elems: Vec<Value>) -> (VarGcRef, usize) {
        let len = elems.len();
        let block = heap.alloc_var_block(len * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `len` Values; `write` moves each value in without dropping.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in elems.into_iter().enumerate() {
            unsafe { base.add(i).write(v); }
        }
        (block, len)
    }

    /// Allocate a packed POD block (`BlockType::ArrayPrim`) and copy `data` into it.
    pub(super) fn alloc_packed<T: Copy>(heap: &dyn MagrGC, data: &[T]) -> VarGcRef {
        let block = heap.alloc_var_block(std::mem::size_of_val(data), BlockType::ArrayPrim);
        if !data.is_empty() {
            // SAFETY: block payload sized `size_of_val(data)`, 8-aligned ≥ align_of::<T>();
            // src/dst are distinct, non-overlapping regions of `data.len()` `T`s.
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), block.payload_as_ptr::<T>(), data.len()); }
        }
        block
    }

    /// Allocate a block of `bytes` bytes of `bt` and copy the first `bytes` of `src`'s payload
    /// into it (deep copy of a block-backed array). The source is read with atomic chunked loads
    /// ([`Cells::copy_out`]) — another thread may be writing it; the fresh block is written plainly
    /// (nothing can see it yet).
    pub(super) fn alloc_block_copy(heap: &dyn MagrGC, src: &VarGcRef, bytes: usize, bt: BlockType) -> VarGcRef {
        let block = heap.alloc_var_block(bytes, bt);
        if bytes > 0 {
            // SAFETY: a fresh block of `bytes` bytes, reachable from nothing yet.
            let dst = unsafe { Self::slice_of_mut::<u8>(&block, bytes) };
            Self::cells_of(src, bytes).copy_out(0, dst);
        }
        block
    }

    /// An untyped MIXED array (element type unknown) — Rust-synthesized arrays.
    #[inline]
    pub fn new(heap: &dyn MagrGC, elems: Vec<Value>) -> Self {
        let len = elems.len();
        Self { element_type: ElemType::empty(), backing: Self::pack_backing_iter(heap, ElemKind::Mixed, len, elems.into_iter()) }
    }

    /// A FROZEN array: `elems` written once, read-only afterwards (closure environments, the
    /// `ref_word` kind-7 value box).
    #[inline]
    pub fn frozen(heap: &dyn MagrGC, elems: Vec<Value>) -> Self {
        let (block, len) = Self::alloc_frozen(heap, elems);
        Self { element_type: ElemType::empty(), backing: ArrayBacking::Frozen { block, len } }
    }

    /// Allocate a FROZEN array on `heap` — a closure environment (captures are only ever read).
    /// `Value::Null` under strict-OOM refusal, like every allocation.
    pub fn alloc_frozen_value(heap: &dyn MagrGC, elems: Vec<Value>) -> Value {
        heap.alloc_array_obj(Self::frozen(heap, elems))
    }

    /// An array of `element_type` (name-only mode choice, [`ElemKind::of`]).
    #[inline]
    pub fn typed(heap: &dyn MagrGC, element_type: &str, elems: Vec<Value>) -> Self {
        let et = ElemType::intern(element_type);
        let len = elems.len();
        Self { element_type: et, backing: Self::pack_backing_iter(heap, et.kind(), len, elems.into_iter()) }
    }

    /// An escape-analysis **stack array**: elements in an arena-owned `Vec`, no GC allocation.
    #[inline]
    pub fn stack_typed(element_type: ElemType, elems: Vec<Value>) -> Self {
        Self { element_type, backing: ArrayBacking::StackVec(elems) }
    }

    /// A `byte[]` straight from an owned `Vec<u8>` (FFI return fast path).
    pub fn from_bytes(heap: &dyn MagrGC, bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let block = Self::alloc_packed(heap, &bytes);
        Self { element_type: ElemType::byte(), backing: ArrayBacking::Bytes { block, len } }
    }

    /// A value-struct array `Point[len]`: one zeroed block of `len * layout.size` bytes (zero =
    /// the default struct, every reference leaf `null`).
    pub fn struct_backed(heap: &dyn MagrGC, element_type: ElemType, len: usize, layout: std::sync::Arc<StructTypeLayout>) -> Self {
        let block = heap.alloc_var_block(len * layout.size, BlockType::ArrayStruct);
        Self { element_type, backing: ArrayBacking::StructBytes { len, block, layout } }
    }
}

// ── 按模式建 backing：直接写进新 GC 块，不经中转 `Vec` ──
impl ArrayObj {
    /// Build the backing of mode `kind` **directly in its GC block** from an element iterator
    /// that yields exactly `len` values (debug-asserted; a short iterator leaves the block's zero
    /// = the mode's default).
    ///
    /// GC safety: the block is allocated and written here, before the caller allocates the
    /// `ArrayObj` header (`alloc_array_obj`). The fresh block is reachable from no root until the
    /// header points at it, and only an unwired unit-test heap collects inside an allocation —
    /// so the one allocation that can happen in between (boxing a value that does not fit a REF
    /// word / MIXED cell, done after the element loop) is safe on every VM heap.
    pub(super) fn pack_backing_iter(
        heap: &dyn MagrGC, kind: ElemKind, len: usize, elems: impl Iterator<Item = Value>,
    ) -> ArrayBacking {
        macro_rules! packed {
            ($variant:ident, $ty:ty, $conv:expr) => {{
                let block = heap.alloc_var_block(len * std::mem::size_of::<$ty>(), BlockType::ArrayPrim);
                // SAFETY: fresh block sized for exactly `len` `$ty`s, reachable from nothing yet.
                let dst = unsafe { Self::slice_of_mut::<$ty>(&block, len) };
                let mut n = 0usize;
                for (slot, x) in dst.iter_mut().zip(elems) { *slot = $conv(&x); n += 1; }
                debug_assert_eq!(n, len, "pack_backing_iter: iterator yielded {n} of {len} elements");
                ArrayBacking::$variant { block, len }
            }};
        }
        match kind {
            ElemKind::Bytes => packed!(Bytes, u8, |x: &Value| if let Value::I64(n) = x { *n as u8 } else { prim_value_mismatch(x, "byte[]", "pack_backing"); 0 }),
            ElemKind::Chars => packed!(Chars, char, |x: &Value| if let Value::Char(c) = x { *c } else { prim_value_mismatch(x, "char[]", "pack_backing"); '\0' }),
            ElemKind::Bool => packed!(Bool, bool, |x: &Value| { if !matches!(x, Value::Bool(_)) { prim_value_mismatch(x, "bool[]", "pack_backing"); } matches!(x, Value::Bool(true)) }),
            ElemKind::I32 => packed!(I32, i32, |x: &Value| if let Value::I64(n) = x { *n as i32 } else { prim_value_mismatch(x, "int[]", "pack_backing"); 0 }),
            ElemKind::I64 => packed!(I64, i64, |x: &Value| if let Value::I64(n) = x { *n } else { prim_value_mismatch(x, "long[]", "pack_backing"); 0 }),
            ElemKind::F64 => packed!(F64, f64, |x: &Value| if let Value::F64(f) = x { *f } else { prim_value_mismatch(x, "double[]", "pack_backing"); 0.0 }),
            ElemKind::Ref => Self::pack_refs(heap, len, elems),
            ElemKind::Mixed => Self::pack_cells(heap, len, elems),
        }
    }

    fn pack_refs(heap: &dyn MagrGC, len: usize, elems: impl Iterator<Item = Value>) -> ArrayBacking {
        use std::sync::atomic::Ordering;
        let block = heap.alloc_var_block(len * REF_STRIDE, BlockType::ArrayRef);
        let mut pending: Vec<(usize, Value)> = Vec::new();
        {
            let c = Self::cells_of(&block, len * REF_STRIDE);
            let mut n = 0usize;
            for v in elems.take(len) {
                match ref_word::encode(&v) {
                    Some(w) => c.store_word(n * REF_STRIDE, w, Ordering::Relaxed),
                    None => pending.push((n, v)),
                }
                n += 1;
            }
            debug_assert_eq!(n, len, "pack_backing_iter: iterator yielded {n} of {len} elements");
        }
        for (i, v) in pending {
            Self::init_boxed(&block, len * REF_STRIDE, i * REF_STRIDE, &v);
        }
        ArrayBacking::Refs { block, len }
    }

    fn pack_cells(heap: &dyn MagrGC, len: usize, elems: impl Iterator<Item = Value>) -> ArrayBacking {
        use std::sync::atomic::Ordering;
        let block = heap.alloc_var_block(len * CELL_STRIDE, BlockType::ArrayCell);
        let mut pending: Vec<(usize, Value)> = Vec::new();
        {
            let c = Self::cells_of(&block, len * CELL_STRIDE);
            let mut n = 0usize;
            for v in elems.take(len) {
                match tparam_cell::init_words(&v) {
                    Some((w0, w1)) => {
                        c.store_word(n * CELL_STRIDE + 8, w1, Ordering::Relaxed);
                        c.store_word(n * CELL_STRIDE, w0, Ordering::Relaxed);
                    }
                    None => pending.push((n, v)),
                }
                n += 1;
            }
            debug_assert_eq!(n, len, "pack_backing_iter: iterator yielded {n} of {len} elements");
        }
        for (i, v) in pending {
            Self::init_boxed(&block, len * CELL_STRIDE, i * CELL_STRIDE, &v);
        }
        ArrayBacking::Cells { block, len, zero: 0 }
    }

    /// A fresh element that needs a box: box it (ambient heap) and write the kind-7 word. Without
    /// a heap (a heap-less unit test) the element stays `null` — debug-asserted.
    #[cold]
    fn init_boxed(block: &VarGcRef, bytes: usize, off: usize, v: &Value) {
        match ref_word::alloc_box_ambient(v) {
            Ok(b) => Self::cells_of(block, bytes)
                .store_word(off, ref_word::encode_boxed(&b), std::sync::atomic::Ordering::Release),
            Err(e) => debug_assert!(false, "array element {v:?} needs a box: {e}"),
        }
    }

    /// An `element_type[len]` whose every element is `fill` (`ArrayNew` /
    /// `Array.CreateInstance` — `fill` is the element type's default), in mode `kind`. When
    /// `fill` is the zeroed block's value for that mode (packed `0` / `'\0'` / `false` / `+0.0`,
    /// `null` for REF / MIXED, or a primitive zero for MIXED — recorded as the array's `zero`)
    /// the fresh block **is** the array; otherwise each slot is written once in place.
    pub fn filled(heap: &dyn MagrGC, element_type: ElemType, kind: ElemKind, len: usize, fill: Value) -> Self {
        let zero = match (kind, &fill) {
            (ElemKind::Bytes | ElemKind::I32 | ElemKind::I64, Value::I64(0)) => true,
            (ElemKind::Chars, Value::Char('\0')) => true,
            (ElemKind::Bool, Value::Bool(false)) => true,
            (ElemKind::F64, Value::F64(f)) => f.to_bits() == 0,
            (ElemKind::Ref | ElemKind::Mixed, Value::Null) => true,
            (ElemKind::Mixed, v) => tparam_cell::zero_kind_of(v) != 0,
            _ => false,
        };
        let backing = if zero {
            let (bytes, bt) = match kind {
                ElemKind::Bytes | ElemKind::Bool => (len, BlockType::ArrayPrim),
                ElemKind::I32 | ElemKind::Chars => (len * 4, BlockType::ArrayPrim),
                ElemKind::I64 | ElemKind::F64 => (len * 8, BlockType::ArrayPrim),
                ElemKind::Ref => (len * REF_STRIDE, BlockType::ArrayRef),
                ElemKind::Mixed => (len * CELL_STRIDE, BlockType::ArrayCell),
            };
            let block = heap.alloc_var_block(bytes, bt);
            match kind {
                ElemKind::Bytes => ArrayBacking::Bytes { block, len },
                ElemKind::Bool => ArrayBacking::Bool { block, len },
                ElemKind::I32 => ArrayBacking::I32 { block, len },
                ElemKind::Chars => ArrayBacking::Chars { block, len },
                ElemKind::I64 => ArrayBacking::I64 { block, len },
                ElemKind::F64 => ArrayBacking::F64 { block, len },
                ElemKind::Ref => ArrayBacking::Refs { block, len },
                ElemKind::Mixed => ArrayBacking::Cells { block, len, zero: tparam_cell::zero_kind_of(&fill) },
            }
        } else {
            Self::pack_backing_iter(heap, kind, len, std::iter::repeat_n(fill, len))
        };
        Self { element_type, backing }
    }

    /// [`Self::filled`] in the mode the element type's name selects.
    #[inline]
    pub fn typed_filled(heap: &dyn MagrGC, element_type: ElemType, len: usize, fill: Value) -> Self {
        Self::filled(heap, element_type, element_type.kind(), len, fill)
    }

    /// An array of mode `kind` over an element iterator of known length (array literals read
    /// straight from their source registers).
    pub fn from_iter_kind(heap: &dyn MagrGC, element_type: ElemType, kind: ElemKind, len: usize, elems: impl Iterator<Item = Value>) -> Self {
        Self { element_type, backing: Self::pack_backing_iter(heap, kind, len, elems) }
    }

    /// [`Self::from_iter_kind`] in the mode the element type's name selects.
    #[inline]
    pub fn typed_iter(heap: &dyn MagrGC, element_type: ElemType, len: usize, elems: impl Iterator<Item = Value>) -> Self {
        Self::from_iter_kind(heap, element_type, element_type.kind(), len, elems)
    }
}

/// Which mode an element type selects — the single key the constructors dispatch on (cached per
/// interned [`ElemType`]). Name-only: the packed primitives (only widths that round-trip
/// losslessly through `get_boxed` / `set_boxed`), REF for the reference types a name alone
/// proves (`string`, `object`, any array type), MIXED for everything else. `objops::array`
/// upgrades a MIXED class / interface / delegate element type to REF with the type registry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElemKind { Bool, Bytes, I32, I64, Chars, F64, Ref, Mixed }

impl ElemKind {
    #[inline]
    pub fn of(element_type: &str) -> Self {
        match element_type {
            "byte" | "u8" => Self::Bytes,
            "char" => Self::Chars,
            "bool" => Self::Bool,
            // fits i32 signed range (i8/i16/i32 and u16 ≤ 65535).
            "sbyte" | "i8" | "short" | "i16" | "int" | "i32" | "ushort" | "u16" => Self::I32,
            // 64-bit (uint/u32 fit i64; u64 keeps existing i64-store semantics).
            "long" | "i64" | "uint" | "u32" | "ulong" | "u64" | "isize" | "usize" => Self::I64,
            "double" | "float" | "f32" | "f64" => Self::F64,
            "string" | "str" | "Std.String" | "object" | "Std.Object" => Self::Ref,
            n if n.ends_with("[]") || n.ends_with("[]?") => Self::Ref,
            _ => Self::Mixed,
        }
    }

    /// Whether this is a packed primitive mode.
    #[inline]
    pub fn is_prim(self) -> bool {
        !matches!(self, Self::Ref | Self::Mixed)
    }
}
