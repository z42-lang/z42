//! ArrayObj / ArrayDesc + construction. Element reads and writes are in `array_elems`, views /
//! GC / copies in `array_access`.
//!
//! An array is **one** block of the GC's variable-length region (object model R9), laid out like
//! an object: the 16 B block header — GC word, then the kind word, which holds the array's
//! [`ArrayDesc`] — and the elements right after it. No separate header object, no second block.
//!
//! ```text
//!   GcWord (8 B) │ &ArrayDesc (8 B) │ elements …                       (every mode but STRUCT)
//!   GcWord (8 B) │ &ArrayDesc (8 B) │ layout word (8 B) │ elements …   (STRUCT)
//!   └─ block header (16 B) ────────┘ └─ payload: GcWord::size bytes ─┘
//! ```
//!
//! The descriptor is interned per element type and mode (`ElemType::array_desc`, process
//! lifetime): the element type (non-erased reflection — `arr.GetType().GetElementType()`), the
//! mode and MIXED's zero kind. The descriptor is 128-aligned and the kind word carries the mode and
//! the zero kind in its low bits, so element access, the length and the JIT's data queries read
//! only the array's own header. The length is not stored: it is the payload length divided by the
//! element width (a power of two the mode gives). A value-struct array's element layout is per array (two load contexts can give
//! one name two layouts), so it is the one mode with a word of its own: a counted
//! `Arc<StructTypeLayout>` at the start of the payload, released by the heap's drop glue.
//!
//! Elements are stored per mode, chosen at allocation (object model R3) — every mutable cell on
//! the heap ≤ 8 B, read and written with one same-width atomic access:
//!
//! | mode | block type | element | read / write |
//! |---|---|---|---|
//! | PRIM (`bool` / `byte` / `int` / `long` / `char` / `double`) | `ArrayPrim` | packed primitive, natural width | same-width relaxed atomics |
//! | REF | `ArrayRef` | 8 B self-describing reference word (`ref_word`) | acquire / release, swap for SATB while marking |
//! | MIXED | `ArrayCell` | 16 B type-parameter cell (`tparam_cell`: tag word + payload word) | the cell protocol, lock-free |
//! | STRUCT | `ArrayStruct` | value-struct blob, reference leaves are reference words (`struct_blob`) | each leaf by its cell |
//! | FROZEN | `ArrayValue` | 16 B `Value`, written once before publication, read-only afterwards | plain reads |
//!
//! Arrays are fixed-length: nothing resizes a block in place (`List<T>` grows by allocating a new
//! backing array and copying). An escape-analysis stack array is not an `ArrayObj` at all — it is
//! a `Vec<Value>` in the frame arena (`interp::stack_alloc::StackArray`).

#![allow(unused_imports)]
use super::*;
use std::ptr::NonNull;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::ArrRef;
use crate::gc::var_region::{BlockType, GcBlockHeader, GcWord, VarGcRef};
use crate::gc::heap::MagrGC;

/// An array's storage mode (see the module docs). The packed primitive modes carry their
/// element kind; MIXED's zero kind lives in the descriptor ([`ArrayDesc::zero`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ArrayMode { Bool, Bytes, I32, I64, Chars, F64, Ref, Mixed, Frozen, Struct }

impl ArrayMode {
    /// The mode an [`ElemKind`] selects (MIXED for `Mixed`).
    #[inline]
    pub fn of_kind(kind: ElemKind) -> Self {
        match kind {
            ElemKind::Bool => Self::Bool,
            ElemKind::Bytes => Self::Bytes,
            ElemKind::I32 => Self::I32,
            ElemKind::I64 => Self::I64,
            ElemKind::Chars => Self::Chars,
            ElemKind::F64 => Self::F64,
            ElemKind::Ref => Self::Ref,
            ElemKind::Mixed => Self::Mixed,
        }
    }

    /// Whether this is a packed primitive mode.
    #[inline(always)]
    pub fn is_prim(self) -> bool {
        (self as u8) <= (Self::F64 as u8)
    }

    /// `log2` of the element width (every mode but STRUCT, whose width is the layout's size).
    #[inline(always)]
    pub const fn shift(self) -> u8 {
        match self {
            Self::Bool | Self::Bytes | Self::Struct => 0,
            Self::I32 | Self::Chars => 2,
            Self::I64 | Self::F64 | Self::Ref => 3,
            Self::Mixed | Self::Frozen => 4,
        }
    }

    /// The mode from the low bits of an array's kind word ([`ArrayDesc::kind_word`]).
    #[inline(always)]
    fn from_bits(b: usize) -> Self {
        match b & 15 {
            0 => Self::Bool, 1 => Self::Bytes, 2 => Self::I32, 3 => Self::I64, 4 => Self::Chars,
            5 => Self::F64, 6 => Self::Ref, 7 => Self::Mixed, 8 => Self::Frozen, _ => Self::Struct,
        }
    }
}

/// Bits of an array's kind word below the descriptor's alignment: the mode (low 4 bits) and the
/// MIXED zero kind (next 3), so the hot paths read an array's shape from its own header without a
/// dependent load of the descriptor.
const KIND_TAG_MASK: usize = 127;

/// What an array's kind word points at: element type, mode and MIXED zero kind. Interned per
/// element type and mode (`ElemType::array_desc`), never freed — the block header holds a plain
/// `&'static` (no reference count on the allocation path), tagged with the mode and the zero kind
/// in the low bits the 128 B alignment leaves free ([`Self::kind_word`]).
#[derive(Debug)]
#[repr(C, align(128))]
pub struct ArrayDesc {
    pub mode: ArrayMode,
    /// MIXED: the primitive kind a never-written cell (tag word `0`) reads as — the zero of `T`
    /// when `T` was a primitive at allocation (`new T[n]` with `T = int`), `0` = `null` — so the
    /// default fill writes nothing and unused capacity (a `List<int>`'s tail) is never touched.
    /// `0` in every other mode.
    pub zero: u8,
    pub elem_type: ElemType,
}

/// Byte stride of a REF element.
pub const REF_STRIDE: usize = 8;
/// Byte stride of a MIXED element (tag word + payload word).
pub const CELL_STRIDE: usize = 16;
/// STRUCT: the layout word ahead of the elements.
pub(super) const STRUCT_HEAD: usize = 8;
/// Padding after the descriptor word so the payload starts at the block's `DATA_OFFSET` on every
/// target (4 bytes on 32-bit ones, none on 64-bit).
const DESC_WORD_PAD: usize = 16 - std::mem::size_of::<GcWord>() - std::mem::size_of::<usize>();

const _: () = assert!(std::mem::size_of::<Value>() == 16);

impl ArrayDesc {
    /// Slots per element type (`ElemTypeInfo::descs`): one per mode, MIXED once per zero kind.
    pub(super) const SLOTS: usize = 16;

    /// The cache slot of `(mode, zero)`.
    #[inline(always)]
    pub(super) fn slot(mode: ArrayMode, zero: u8) -> usize {
        match mode {
            ArrayMode::Mixed if zero != 0 => {
                debug_assert!((zero as usize) < Self::SLOTS - 10, "MIXED zero kind {zero} out of range");
                10 + (zero as usize).min(Self::SLOTS - 11)
            }
            m => m as usize,
        }
    }

    pub(super) fn new(elem_type: ElemType, mode: ArrayMode, zero: u8) -> Self {
        debug_assert!(zero < 8, "MIXED zero kind {zero} does not fit the kind word's tag");
        Self { mode, zero: if mode == ArrayMode::Mixed { zero & 7 } else { 0 }, elem_type }
    }

    /// The value an array's header kind word holds: this descriptor's address tagged with the
    /// mode (bits 0–3) and the zero kind (bits 4–6).
    #[inline]
    fn kind_word(&'static self) -> *const ArrayDesc {
        let tag = self.mode as usize | (self.zero as usize) << 4;
        (self as *const ArrayDesc).map_addr(|a| a | tag)
    }

    /// The block type an array of this descriptor is allocated as (what the tracer dispatches on).
    #[inline]
    pub fn block_type(&self) -> BlockType {
        match self.mode {
            ArrayMode::Ref => BlockType::ArrayRef,
            ArrayMode::Mixed => BlockType::ArrayCell,
            ArrayMode::Frozen => BlockType::ArrayValue,
            ArrayMode::Struct => BlockType::ArrayStruct,
            _ => BlockType::ArrayPrim,
        }
    }

    /// Element width in bytes (not for STRUCT).
    #[inline(always)]
    pub fn width(&self) -> usize {
        1 << self.mode.shift()
    }

    /// Payload bytes of a `len`-element array (`layout`: STRUCT only) — `None` past the block
    /// size limit (`u32` payload length), which the allocator reports as out of memory.
    pub fn payload_bytes(&self, len: usize, layout: Option<&StructTypeLayout>) -> Option<usize> {
        let bytes = match (self.mode, layout) {
            (ArrayMode::Struct, Some(l)) => len.checked_mul(l.size)?.checked_add(STRUCT_HEAD)?,
            (ArrayMode::Struct, None) => return None,
            _ => len.checked_mul(self.width())?,
        };
        (bytes <= u32::MAX as usize).then_some(bytes)
    }
}

/// The heap payload behind `Value::Array` — **one** 16 B header and the elements, as one
/// dynamically sized value laid over the array's whole block (see the module docs).
///
/// `&ArrayObj` is a fat reference whose metadata is the payload length, so every element access
/// stays inside the reference's own range (Stacked Borrows: the retag spans `size_of_val`). It is
/// built only from a pointer carrying the whole block's provenance ([`ArrayObj::from_block`]:
/// `ArrRef::borrow`, the allocator, the drop glue). `ArrayObj` is never owned by value.
#[repr(C)]
pub struct ArrayObj {
    /// The block's GC word (owned by the region).
    gc: GcWord,
    /// The block header's kind word: the interned descriptor, tagged with the mode and the zero
    /// kind ([`ArrayDesc::kind_word`]); written before the array is published, never changed.
    kind: *const ArrayDesc,
    _pad: [std::mem::MaybeUninit<u8>; DESC_WORD_PAD],
    /// The payload: STRUCT's layout word, then the elements. `UnsafeCell` because elements are
    /// written through `&self` (atomic cell stores).
    pub(super) payload: [std::cell::UnsafeCell<u8>],
}

const _: () = assert!(std::mem::offset_of!(ArrayObj, kind) == crate::gc::var_region::KIND_WORD_OFFSET);
const _: () = assert!(std::mem::align_of::<ArrayDesc>() > KIND_TAG_MASK);

// SAFETY: the header's fields are `Send + Sync` (atomics, a `&'static` descriptor); elements are
// only ever accessed through atomic cell views (`Cells`), except FROZEN ones, which are written
// once before the array is published and only read afterwards. STRUCT's layout word is written
// before publication too, and what it owns (`StructTypeLayout`) is `Send + Sync`.
unsafe impl Send for ArrayObj {}
unsafe impl Sync for ArrayObj {}

impl std::fmt::Debug for ArrayObj {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArrayObj")
            .field("element_type", &self.desc().elem_type)
            .field("mode", &self.mode())
            .field("len", &self.len())
            .finish()
    }
}

impl ArrayObj {
    /// The array over the block at `header` whose payload is `payload_size` bytes, as a fat raw
    /// pointer whose metadata is the payload length.
    ///
    /// # Safety
    /// `header` carries the provenance of the whole block, whose capacity covers `size_of_val` of
    /// the result (every size class / detached block does). The descriptor word is installed
    /// before anything reads through the pointer.
    #[inline(always)]
    pub(crate) unsafe fn from_block(header: NonNull<u8>, payload_size: usize) -> *mut ArrayObj {
        std::ptr::slice_from_raw_parts_mut(header.as_ptr(), payload_size) as *mut ArrayObj
    }

    /// Install `desc` (and, for STRUCT, the counted `layout`) into the fresh, unpublished block at
    /// `header` and return the array over it.
    ///
    /// # Safety
    /// `header` heads a zeroed block (whole-block provenance) of exactly `payload` bytes nothing
    /// else can reach yet, whose kind word is null; `payload` is `desc.payload_bytes(len, layout)`.
    pub(crate) unsafe fn install<'a>(
        header: NonNull<u8>, payload: usize, desc: &'static ArrayDesc, layout: Option<Arc<StructTypeLayout>>,
    ) -> &'a ArrayObj {
        // SAFETY: per the contract; place projections on the raw pointer — no reference to the
        // header is formed while it is being written.
        unsafe {
            let p = Self::from_block(header, payload);
            std::ptr::addr_of_mut!((*p).kind).write(desc.kind_word());
            if let Some(l) = layout {
                debug_assert_eq!(desc.mode, ArrayMode::Struct);
                Self::layout_word(p).write(Arc::into_raw(l));
            }
            &*p
        }
    }

    /// STRUCT's layout word (the payload's first word).
    #[inline(always)]
    fn layout_word(p: *mut ArrayObj) -> *mut *const StructTypeLayout {
        // SAFETY: a STRUCT payload starts with the 8-aligned layout word, in range of `p`.
        unsafe { std::cell::UnsafeCell::raw_get(std::ptr::addr_of!((*p).payload).cast::<std::cell::UnsafeCell<u8>>()).cast() }
    }

    /// Release what a STRUCT array owns — its layout — so the heap's drop glue can run once per
    /// dead block. No-op for a layout word never written (an allocation refunded early).
    ///
    /// # Safety
    /// `p` is a STRUCT array over its whole block, destroyed exactly once; nothing reads it after.
    pub(crate) unsafe fn destroy_struct(p: *mut ArrayObj) {
        // SAFETY: per the contract.
        unsafe {
            let w = Self::layout_word(p);
            let l = w.read();
            if !l.is_null() {
                drop(Arc::from_raw(l));
                w.write(std::ptr::null());
            }
        }
    }

    /// The array's descriptor.
    #[inline(always)]
    pub fn desc(&self) -> &'static ArrayDesc {
        // SAFETY: the kind word is a tagged `&'static ArrayDesc` (see `install`); untagging keeps
        // the pointer's provenance.
        unsafe { &*self.kind.map_addr(|a| a & !KIND_TAG_MASK) }
    }

    /// The array's mode — from the header's kind word, no descriptor load.
    #[inline(always)]
    pub fn mode(&self) -> ArrayMode {
        ArrayMode::from_bits(self.kind.addr())
    }

    /// MIXED: the primitive kind a never-written cell reads as (`0` = `null`); `0` otherwise.
    #[inline(always)]
    pub fn zero(&self) -> u8 {
        ((self.kind.addr() >> 4) & 7) as u8
    }

    /// Element width in bytes (not for STRUCT).
    #[inline(always)]
    pub fn width(&self) -> usize {
        1 << self.mode().shift()
    }

    /// Element type FQ name (e.g. "int" / "geometry.Point"); empty = unknown (Rust-synthesized
    /// arrays like reflection result sets, closure environments).
    #[inline(always)]
    pub fn element_type(&self) -> ElemType {
        self.desc().elem_type
    }

    /// STRUCT: the element layout (borrowed — it lives as long as the array). `None` otherwise.
    #[inline]
    pub fn struct_layout_ref(&self) -> Option<&StructTypeLayout> {
        if self.mode() != ArrayMode::Struct {
            return None;
        }
        // SAFETY: a STRUCT array's layout word was installed before publication and stays until
        // the block dies (the reference cannot outlive the array).
        let l = unsafe { Self::layout_word(self as *const Self as *mut Self).read() };
        // SAFETY: a non-null layout word is a live `Arc` the array owns.
        (!l.is_null()).then(|| unsafe { &*l })
    }

    /// The `struct[]` element layout as a counted `Arc` (`None` for any other mode).
    #[inline]
    pub fn struct_layout(&self) -> Option<Arc<StructTypeLayout>> {
        let l = self.struct_layout_ref()?;
        // SAFETY: `l` came from `Arc::into_raw` and the array holds that count while we bump it.
        unsafe {
            Arc::increment_strong_count(l as *const StructTypeLayout);
            Some(Arc::from_raw(l as *const StructTypeLayout))
        }
    }

    /// Byte offset of element 0 within the payload.
    #[inline(always)]
    fn elems_off(&self) -> usize {
        if self.mode() == ArrayMode::Struct { STRUCT_HEAD } else { 0 }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        let bytes = self.payload.len();
        let mode = self.mode();
        if mode == ArrayMode::Struct {
            return match self.struct_layout_ref() {
                Some(l) if l.size != 0 => (bytes - STRUCT_HEAD) / l.size,
                _ => 0,
            };
        }
        bytes >> mode.shift()
    }

    #[inline]
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Bytes of element storage (`len × element width`).
    #[inline]
    pub fn elem_storage_bytes(&self) -> usize {
        self.payload.len() - self.elems_off()
    }

    /// Element 0's address (whole-block provenance — derived from `self`'s own range).
    #[inline(always)]
    pub(super) fn elems_ptr(&self) -> *mut u8 {
        // SAFETY: `elems_off() <= payload.len()`, in range of `&self`.
        unsafe { std::cell::UnsafeCell::raw_get(self.payload.as_ptr()).add(self.elems_off()) }
    }

    /// Atomic loads and stores over the element area.
    #[inline(always)]
    pub(super) fn cells(&self) -> Cells<'_> {
        // SAFETY: the element area is in range of `&self`, 8-aligned (the payload starts at the
        // block's 8-aligned `DATA_OFFSET`, STRUCT's layout word is 8 B), interior-mutable, and
        // every concurrent access to it is an atomic cell access.
        unsafe { Cells::from_raw(self.elems_ptr(), self.elem_storage_bytes()) }
    }

    /// FROZEN: the elements as `Value`s (written once before publication, read-only after).
    #[inline]
    pub(super) fn frozen_values(&self) -> &[Value] {
        debug_assert_eq!(self.mode(), ArrayMode::Frozen);
        // SAFETY: a FROZEN payload is `len` initialized `Value`s, 8-aligned, in range of `&self`,
        // never written after publication.
        unsafe { std::slice::from_raw_parts(self.elems_ptr() as *const Value, self.payload.len() / 16) }
    }

    /// Element `i`'s address in a FROZEN array under construction.
    #[inline]
    fn frozen_slot(&self, i: usize) -> *mut Value {
        debug_assert!(i < self.len());
        // SAFETY: in range (`i < len`).
        unsafe { (self.elems_ptr() as *mut Value).add(i) }
    }
}

/// 一个 `Value` 与它要写进的**基元数组**类型不符时的信号（make-silent-fallbacks-signal）。
///
/// 「类型不符」回落存 0：`0` 是程序**完全无法与合法写入区分**的答案 —— 一个 `int[]` 里的 `0`
/// 既可能是用户写的，也可能是这里吞掉的一次类型错误。取 `debug_assert!` 而不是 `bail!`：
/// - golden 语料默认跑 debug VM ⇒ **CI 本身就是探测器**；
/// - 「类型不符」是编译器该在 `ArraySet` / `ArrayNewLit` 站点转换/拆箱掉的事，**不是用户的错**，
///   所以 release 放行（不拿用户崩溃换诊断能力）。
///
/// ⚠️ 这只对 IR 站点（`ArraySet` / `ArrayNewLit`）成立。**无类型 API**（`Std.Array.SetValue(Object,
/// int)`、跨元素类型的 `Array.CopyRange`）的值是用户给的、没有编译器站点能转换，它们改走
/// [`ArrayObj::try_set_boxed`] / 前置逐元素校验，**两个 profile 都报错**。
#[inline]
pub(super) fn prim_value_mismatch(val: &Value, backing: &str, site: &str) {
    debug_assert!(false,
        "array {site}: {backing} array got a non-matching Value ({val:?}) — stored a zero. \
         A silent zero is indistinguishable from a legitimate write.\n\
         → From an IR `ArraySet` / `ArrayNewLit`: the COMPILER should have converted/unboxed at \
         that site — that is the bug to fix.\n\
         → From an UNTYPED API (`Std.Array.SetValue(Object, int)`, or `Array.CopyRange` across \
         element types): those paths validate up front and raise instead (`try_set_boxed`) — \
         reaching here from them means a validation path was bypassed.");
}

// ── 构造：在数组自己的块里建好再发布 ──
impl ArrayObj {
    /// The one allocation path: a `len`-element array of `desc` (`layout`: STRUCT only), zeroed —
    /// every mode's default — then `init` on the unpublished array. `Value::Null` when the heap
    /// refuses (strict OOM, or a payload past the block size limit).
    #[inline]
    fn alloc(
        heap: &dyn MagrGC, desc: &'static ArrayDesc, len: usize, layout: Option<Arc<StructTypeLayout>>,
        init: &mut dyn FnMut(&ArrayObj),
    ) -> Value {
        heap.alloc_array_with(desc, len, layout, init)
    }

    /// An untyped MIXED array (element type unknown) — Rust-synthesized arrays.
    pub fn alloc_untyped(heap: &dyn MagrGC, elems: Vec<Value>) -> Value {
        let len = elems.len();
        Self::alloc_from_iter(heap, ElemType::empty(), ElemKind::Mixed, len, elems.into_iter())
    }

    /// A FROZEN array: `elems` written once, read-only afterwards (closure environments, the
    /// `ref_word` kind-7 value box — which may hold a stack handle no cell can encode).
    pub fn alloc_frozen(heap: &dyn MagrGC, elems: &[Value]) -> Value {
        let desc = ElemType::empty().array_desc(ArrayMode::Frozen, 0);
        Self::alloc(heap, desc, elems.len(), None, &mut |a| {
            for (i, v) in elems.iter().enumerate() {
                // SAFETY: a fresh FROZEN block sized for `elems.len()` Values; nothing reads it yet.
                unsafe { a.frozen_slot(i).write(*v) };
            }
        })
    }

    /// An array of `element_type` (name-only mode choice, [`ElemKind::of`]).
    pub fn alloc_typed(heap: &dyn MagrGC, element_type: &str, elems: Vec<Value>) -> Value {
        let et = ElemType::intern(element_type);
        let len = elems.len();
        Self::alloc_from_iter(heap, et, et.kind(), len, elems.into_iter())
    }

    /// A `byte[]` straight from bytes (FFI return fast path).
    pub fn alloc_bytes(heap: &dyn MagrGC, bytes: &[u8]) -> Value {
        let desc = ElemType::byte().array_desc(ArrayMode::Bytes, 0);
        Self::alloc(heap, desc, bytes.len(), None, &mut |a| {
            a.cells().copy_in(0, bytes);
        })
    }

    /// A zeroed value-struct array `Point[len]` (zero = the default struct, every reference leaf
    /// `null`).
    pub fn alloc_struct(heap: &dyn MagrGC, element_type: ElemType, len: usize, layout: Arc<StructTypeLayout>) -> Value {
        let desc = element_type.array_desc(ArrayMode::Struct, 0);
        Self::alloc(heap, desc, len, Some(layout), &mut |_| {})
    }

    /// An `element_type[len]` whose every element is `fill` (`ArrayNew` / `Array.CreateInstance`
    /// — `fill` is the element type's default), in mode `kind`. When `fill` is the zeroed
    /// block's value for that mode (packed `0` / `'\0'` / `false` / `+0.0`, `null` for REF /
    /// MIXED, or a primitive zero for MIXED — recorded as the descriptor's zero kind) the fresh
    /// block **is** the array; otherwise each element is written once in place.
    pub fn alloc_filled(heap: &dyn MagrGC, element_type: ElemType, kind: ElemKind, len: usize, fill: Value) -> Value {
        let zero = match (kind, &fill) {
            (ElemKind::Bytes | ElemKind::I32 | ElemKind::I64, Value::I64(0)) => true,
            (ElemKind::Chars, Value::Char('\0')) => true,
            (ElemKind::Bool, Value::Bool(false)) => true,
            (ElemKind::F64, Value::F64(f)) => f.to_bits() == 0,
            (ElemKind::Ref | ElemKind::Mixed, Value::Null) => true,
            (ElemKind::Mixed, v) => tparam_cell::zero_kind_of(v) != 0,
            _ => false,
        };
        if zero {
            let z = if kind == ElemKind::Mixed { tparam_cell::zero_kind_of(&fill) } else { 0 };
            let desc = element_type.array_desc(ArrayMode::of_kind(kind), z);
            return Self::alloc(heap, desc, len, None, &mut |_| {});
        }
        Self::alloc_from_iter(heap, element_type, kind, len, std::iter::repeat_n(fill, len))
    }

    /// [`Self::alloc_filled`] in the mode the element type's name selects.
    #[inline]
    pub fn alloc_typed_filled(heap: &dyn MagrGC, element_type: ElemType, len: usize, fill: Value) -> Value {
        Self::alloc_filled(heap, element_type, element_type.kind(), len, fill)
    }

    /// An array of mode `kind` over an element iterator that yields exactly `len` values
    /// (debug-asserted; a short iterator leaves the rest at the mode's default), written straight
    /// into the block. A value that needs a box (a raw primitive reaching `object[]`, a stack
    /// handle in a MIXED cell) is boxed after the element loop.
    pub fn alloc_from_iter(
        heap: &dyn MagrGC, element_type: ElemType, kind: ElemKind, len: usize, elems: impl Iterator<Item = Value>,
    ) -> Value {
        let desc = element_type.array_desc(ArrayMode::of_kind(kind), 0);
        let mut elems = Some(elems);
        Self::alloc(heap, desc, len, None, &mut |a| {
            if let Some(it) = elems.take() {
                a.pack(len, it);
            }
        })
    }

    /// [`Self::alloc_from_iter`] in the mode the element type's name selects.
    #[inline]
    pub fn alloc_typed_iter(heap: &dyn MagrGC, element_type: ElemType, len: usize, elems: impl Iterator<Item = Value>) -> Value {
        Self::alloc_from_iter(heap, element_type, element_type.kind(), len, elems)
    }

    /// Write the first `len` elements of a fresh (unpublished) array from `elems`.
    fn pack(&self, len: usize, elems: impl Iterator<Item = Value>) {
        use std::sync::atomic::Ordering;
        let c = self.cells();
        let mut pending: Vec<(usize, Value)> = Vec::new();
        let mut n = 0usize;
        let mode = self.mode();
        let width = self.width();
        for v in elems.take(len) {
            match mode {
                ArrayMode::Ref => match ref_word::encode(&v) {
                    Some(w) => c.store_word(n * REF_STRIDE, w, Ordering::Relaxed),
                    None => pending.push((n, v)),
                },
                ArrayMode::Mixed => match tparam_cell::init_words(&v) {
                    Some((w0, w1)) => {
                        c.store_word(n * CELL_STRIDE + 8, w1, Ordering::Relaxed);
                        c.store_word(n * CELL_STRIDE, w0, Ordering::Relaxed);
                    }
                    None => pending.push((n, v)),
                },
                // SAFETY: a fresh FROZEN block, nothing reads it yet.
                ArrayMode::Frozen => unsafe { self.frozen_slot(n).write(v) },
                ArrayMode::Struct => debug_assert!(false, "struct[] elements are packed by `write_struct_elem`"),
                _ => {
                    let bits = self.prim_bits(&v, "pack");
                    c.store_bits(n * width, width, bits);
                }
            }
            n += 1;
        }
        debug_assert_eq!(n, len, "ArrayObj::pack: iterator yielded {n} of {len} elements");
        let stride = if mode == ArrayMode::Ref { REF_STRIDE } else { CELL_STRIDE };
        for (i, v) in pending {
            Self::init_boxed(&c, i * stride, &v);
        }
    }

    /// A fresh element that needs a box: box it (ambient heap) and write the kind-7 word. Without
    /// a heap (a heap-less unit test) the element stays `null` — debug-asserted.
    #[cold]
    fn init_boxed(c: &Cells, off: usize, v: &Value) {
        match ref_word::alloc_box_ambient(v) {
            Ok(b) => c.store_word(off, ref_word::encode_boxed(&b), std::sync::atomic::Ordering::Release),
            Err(e) => debug_assert!(false, "array element {v:?} needs a box: {e}"),
        }
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

    /// A **type parameter's resolved argument name** → the packed kind it selects (M13: `new T[n]`
    /// with `T` bound to a primitive packs the array). The name vocabulary is `default_value_for`'s
    /// (field keywords, signature tags, and the FQ wrapper names reflection threads through), so a
    /// reflective `MakeGenericMethod(typeof(int))` and a direct `new T[n]` in `List<int>` pick the
    /// same packing; the width grouping follows [`Self::of`] (`sbyte`/`u16` fit i32, `uint`/`u32`
    /// i64). Everything else — reference, struct, or an open parameter's own literal name — is
    /// `Mixed` and the array stays erased (cells).
    #[inline]
    pub fn of_type_param(name: &str) -> Self {
        match name {
            "byte" | "u8" | "Std.Byte" => Self::Bytes,
            "sbyte" | "i8" | "short" | "i16" | "int" | "i32" | "ushort" | "u16"
            | "Std.SByte" | "Std.Int16" | "Std.Int32" | "Std.UInt16" => Self::I32,
            "long" | "i64" | "uint" | "u32" | "ulong" | "u64" | "isize" | "usize"
            | "Std.UInt32" | "Std.Int64" | "Std.UInt64" => Self::I64,
            "char" | "Std.Char" => Self::Chars,
            "bool" | "Std.Boolean" => Self::Bool,
            "double" | "float" | "f32" | "f64" | "Std.Double" | "Std.Single" => Self::F64,
            _ => Self::Mixed,
        }
    }
}

#[cfg(test)]
impl ArrayObj {
    /// Test-only: a MIXED array in a **leaked** block that no region knows (never marked, swept
    /// or freed) — for heap-less unit tests.
    pub(crate) fn leaked(elems: Vec<Value>) -> ArrRef {
        let desc = ElemType::empty().array_desc(ArrayMode::Mixed, 0);
        let a = Self::leaked_block(desc, elems.len(), None);
        for (i, v) in elems.into_iter().enumerate() { a.borrow().set_boxed(i, v); }
        a
    }

    /// Test-only: a zeroed `struct[]` in a **leaked** block (elements written via
    /// `write_struct_elem`).
    pub(crate) fn struct_leaked(element_type: &str, len: usize, layout: Arc<StructTypeLayout>) -> ArrRef {
        let desc = ElemType::intern(element_type).array_desc(ArrayMode::Struct, 0);
        Self::leaked_block(desc, len, Some(layout))
    }

    /// Test-only: a zeroed array of `desc` in a leaked block.
    pub(crate) fn leaked_block(desc: &'static ArrayDesc, len: usize, layout: Option<Arc<StructTypeLayout>>) -> ArrRef {
        let payload = desc.payload_bytes(len, layout.as_deref()).expect("leaked array too large");
        let v = VarGcRef::alloc_leaked(payload, desc.block_type());
        // SAFETY: a fresh zeroed block of `payload` bytes nobody else can reach.
        unsafe { Self::install(v.header_raw(), payload, desc, layout) };
        ArrRef::from_var(v)
    }
}
