//! Object headers, native payloads and field storage.

#![allow(unused_imports)]
use crate::object::*;
use std::sync::Arc;
use crate::object::string::vstr::Str;
use crate::object::header::{BlockType, GcBlockHeader, GcWord, VarGcRef};
use std::ptr::NonNull;

/// Native backing data for built-in classes.
///
/// Used by `ScriptObject` to hold VM-managed state that should not be
/// directly accessible as a z42 field (i.e. not visible in `slots`).
#[derive(Debug, Clone)]
pub enum NativeData {
    /// No native backing — ordinary user-defined class.
    None,
    /// expose-weak-ref-builtin (D-1a)：包装 GC 弱引用句柄。
    /// 由 `__obj_make_weak` builtin 创建；`__obj_upgrade_weak` 升格回原对象。
    /// 用户视角是 `Std.WeakHandle` 类（无字段）。
    WeakRef(crate::gc::WeakRef),
    /// add-reflection-mvp：`Std.Type` 对象携带的真实类型句柄。
    /// 由 `__obj_get_type` 对 `Value::Object` 创建（存对象 `type_desc` 的
    /// `Arc<TypeDesc>`）；反射 builtins（`__type_fields` / `__type_methods` /
    /// `__type_base` / `__type_generic_args`）据此枚举成员。基础类型/数组的
    /// synthetic Type 无此句柄（`NativeData::None`），成员查询退化为空。
    TypeHandle(Arc<TypeDesc>),
    /// add-load-context-model：`Std.Runtime.LoadContext` 对象携带的
    /// 上下文句柄（root = `ContextId::ROOT`）。`__lctx_*` builtins 据此查
    /// `VmCore.context_registry`。
    LoadContextHandle(crate::metadata::context::ContextId),
    /// add-load-context-model：`Std.Reflection.Assembly` 对象携带的
    /// 程序集句柄（zpkg 运行时投影）。`__asm_*` builtins 据此查注册表。
    AssemblyHandle(crate::metadata::context::AssemblyId),
    /// store-sync-values-in-heap：`Std.Threading.Mutex/RwLock/Channel` 的同步底座。
    /// **不含任何 GC 值**（值是 z42 字段）；句柄对象的槽位被复用时随旧条目 drop。
    Monitor(std::sync::Arc<crate::corelib::monitor::Monitor>),
    // script-first-stringbuilder: removed `StringBuilder(String)` —
    // `Std.Text.StringBuilder` is now a pure z42 script. Variant slot kept open
    // for future native-backed types (Stream / FileHandle / etc.).
}

// ── ScriptObject — unified managed object ───────────────────────────────────
//
// Every class instance (and every boxed value struct / primitive) is one `BlockType::Object`
// block of the variable-length GC region: the 16 B block header — the GC word and the type
// descriptor — followed by the field cells and, for a generic or native-backed instance, one
// meta word (see the type docs below).

/// The cold per-instance native backing of a [`ScriptObject`], boxed only for the few
/// objects that have one (WeakRef / Type / LoadContext / Assembly / Monitor handles).
/// A generic native-backed object keeps its type-arguments handle here too.
#[derive(Debug)]
struct ObjExtras {
    /// Native backing for built-in types.
    native: NativeData,
    /// Per-instance generic type-arguments (interned; empty for non-generic).
    type_args: TypeArgs,
}

impl Default for NativeData {
    fn default() -> Self { NativeData::None }
}

// `ObjMeta` stores the extras box as a tagged raw pointer, which opts out of auto-trait
// propagation; the object stays `Send + Sync` only because what the box owns is.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NativeData>();
};

/// The value of an object's 8 B meta word (cold metadata: `type_args` + native backing):
///
/// - null — non-generic, no native backing;
/// - an even pointer — the interned [`TypeArgs`] entry of a generic instance (no allocation:
///   every instance of one instantiation shares the entry);
/// - an odd pointer — a [`Box<ObjExtras>`] tagged with the low bit (native-backed objects).
///
/// Both pointees are ≥ 8-aligned, so the low bit is free for the tag. A plain value: the word
/// lives (unaligned) at the end of the object's payload and is read / written as a whole; the
/// extras box it may own is freed by `ScriptObject::destroy`.
#[derive(Clone, Copy)]
struct ObjMeta(*const ());

impl ObjMeta {
    const EXTRAS_TAG: usize = 1;
    const NONE: Self = ObjMeta(std::ptr::null());

    #[inline]
    fn extras_ptr(self) -> Option<*mut ObjExtras> {
        if self.0.addr() & Self::EXTRAS_TAG != 0 {
            Some(self.0.map_addr(|a| a & !Self::EXTRAS_TAG) as *mut ObjExtras)
        } else {
            None
        }
    }

    #[inline]
    fn type_args(self) -> &'static [String] {
        self.type_args_handle().as_slice()
    }

    /// The interned handle itself (both word shapes keep one).
    #[inline]
    fn type_args_handle(self) -> TypeArgs {
        if self.0.is_null() {
            return TypeArgs::empty();
        }
        match self.extras_ptr() {
            // SAFETY: a tagged word is an owned, live `Box<ObjExtras>`; its `type_args` is an
            // interned `'static` entry.
            Some(e) => unsafe { (*e).type_args },
            // SAFETY: an untagged non-null word came from `TypeArgs::as_raw`.
            None => unsafe { TypeArgs::from_raw(self.0 as *const TypeArgsInfo) },
        }
    }

    /// The extras box, allocating it (and carrying any type-args handle over) on first use.
    fn extras_or_alloc(&mut self) -> *mut ObjExtras {
        match self.extras_ptr() {
            Some(p) => p,
            None => {
                let type_args = if self.0.is_null() {
                    TypeArgs::empty()
                } else {
                    // SAFETY: an untagged non-null word came from `TypeArgs::as_raw`.
                    unsafe { TypeArgs::from_raw(self.0 as *const TypeArgsInfo) }
                };
                let p = Box::into_raw(Box::new(ObjExtras { native: NativeData::None, type_args }));
                self.0 = (p as *const ()).map_addr(|a| a | Self::EXTRAS_TAG);
                p
            }
        }
    }

    #[inline]
    fn set_type_args(&mut self, ta: TypeArgs) {
        match self.extras_ptr() {
            // SAFETY: owned box, exclusive via the object's `&mut`.
            Some(p) => unsafe { (*p).type_args = ta },
            None if ta.is_empty() => self.0 = std::ptr::null(),
            None => self.0 = ta.as_raw() as *const (),
        }
    }
}

/// Bytes of an object's meta word, when it has one (`gc::var_region::OBJ_META_BIT`).
const META_BYTES: usize = std::mem::size_of::<*const ()>();
const _: () = assert!(META_BYTES <= 8);

/// Padding after the type word so the payload starts at the block's `DATA_OFFSET` on every
/// target (4 bytes on 32-bit ones, none on 64-bit).
const TYPE_WORD_PAD: usize = 16 - std::mem::size_of::<GcWord>() - std::mem::size_of::<usize>();

/// A managed object (object model R6 / R8) — **one** 16 B header and the payload, as one
/// dynamically sized value laid over the object's whole block:
///
/// ```text
///   GcWord (8 B) │ type_desc (8 B) │ field cells … │ [meta word (8 B)]
///   └─ block header (16 B) ───────┘ └─ payload: GcWord::size bytes ───┘
///   ↑ &ScriptObject (fat: address + payload length) ─────────────────────┘
/// ```
///
/// The GC word is the region's (generation + alive bit, mark, type tag, payload length —
/// `object::header::GcWord`), the type word the header's kind word: a heap object is one
/// `BlockType::Object` block of the variable-length region, freed (its `Arc<TypeDesc>` and extras
/// box dropped by the heap's drop glue) when the block is tombstoned. An object outside the heap
/// (a stack-allocated object, one under construction) lives in an [`OwnedObject`], a detached
/// block of the same shape.
///
/// A generic or native-backed instance carries one more word at the very end of its payload (the
/// header's `OBJ_META_BIT` says so; [`ObjMeta`]): the interned type-arguments entry or the extras
/// box. Every other object — the overwhelming majority — is header + fields only.
///
/// `&ScriptObject` is a **fat reference** whose metadata is the payload length, so it covers the
/// header, the fields *and* the meta word: every access stays inside the reference's own range
/// (Stacked Borrows: the retag spans `size_of_val`). It is built only from a pointer that carries
/// the whole block's provenance ([`ScriptObject::from_block`]: `ObjRef::borrow`, `OwnedObject`,
/// the allocators, the drop glue), with the length read from the block header there. Hot
/// structures and ABIs keep thin handles (`ObjRef`, the header address from `ObjRef::as_ptr`),
/// never a `&ScriptObject`.
///
/// Field cells (object model R1–R3): every primitive leaf (incl. inline-struct interior
/// primitive leaves) lives at its composed byte offset, and so does every reference: a direct
/// reference field or an inline-struct reference leaf is an 8 B self-describing reference word
/// (`ref_word`, `ObjectLayout::ref_cells`); a type-parameter field (and every reference field of a
/// synthesized layout) is a 16 B cell whose tag word sits at its own offset and whose payload
/// word sits past the compiler's layout (`tparam_cell`, `ObjectLayout::tparam_cells`). A boxed
/// struct's payload is its struct blob. Field access goes through the methods in
/// `object_fields.rs`, every one an atomic cell access (`Cells`).
#[repr(C)]
pub struct ScriptObject {
    /// The block's GC word (owned by the region; read here only for the meta bit).
    gc: GcWord,
    /// Type descriptor shared across all instances of this class — the block header's kind word.
    /// Written before the object is published, never changed.
    pub type_desc: Arc<TypeDesc>,
    _pad: [std::mem::MaybeUninit<u8>; TYPE_WORD_PAD],
    /// The payload: the field area, then the meta word if the object has one. `UnsafeCell`
    /// because the cells are written through `&self` (atomic cell stores), which a shared
    /// reference to plain bytes would forbid.
    payload: [std::cell::UnsafeCell<u8>],
}

/// The sized prefix of [`ScriptObject`] — exactly its header, same `#[repr(C)]` order: the object
/// view of a block's 16 B [`GcBlockHeader`](crate::object::header::GcBlockHeader). What an
/// object's identity address (`ObjRef::as_ptr`) points at and what the type checks read
/// (`ObjRef::header`). Never built by value: [`ObjHeader::install`] writes the type word into a
/// fresh block.
#[repr(C)]
pub struct ObjHeader {
    gc: GcWord,
    pub type_desc: Arc<TypeDesc>,
    _pad: [std::mem::MaybeUninit<u8>; TYPE_WORD_PAD],
}

// One 16-byte header on every target; the type word is the region's kind word.
const _: () = assert!(std::mem::size_of::<ObjHeader>() == GcBlockHeader::DATA_OFFSET);
const _: () = assert!(std::mem::offset_of!(ObjHeader, type_desc) == crate::object::header::KIND_WORD_OFFSET);
const _: () = assert!(std::mem::size_of::<Arc<TypeDesc>>() == std::mem::size_of::<usize>());

impl ObjHeader {
    /// Write `type_desc` into the kind word of the fresh, unpublished object block at `header`
    /// (the object now owns that reference; the drop glue / `OwnedObject` releases it).
    ///
    /// # Safety
    /// `header` heads a block (whole-block provenance) nothing else can reach yet, whose kind word
    /// is null.
    #[inline]
    pub(crate) unsafe fn install(header: NonNull<u8>, type_desc: Arc<TypeDesc>) {
        let p = header.as_ptr().cast::<ObjHeader>();
        // SAFETY: per the contract; a place projection on the raw pointer — no reference to the
        // header is formed while it is being written.
        unsafe { std::ptr::addr_of_mut!((*p).type_desc).write(type_desc) };
    }
}

/// What JIT-compiled code needs to reach an object's type and field cells from its handle without
/// a call (64-bit targets; `jit::translate::field_ic`): `hdr = handle & HANDLE_ADDR_MASK` is the
/// block header — which is the whole object header — and the field cells follow it; every offset
/// below is from `hdr`. Pinned against the Rust accessors by
/// `object_tests::jit_layout_matches_the_accessors`.
#[cfg(all(feature = "jit", target_pointer_width = "64"))]
pub(crate) mod obj_jit_layout {
    use crate::object::header::GcBlockHeader;

    /// Mask turning an object handle's bits into its block-header address.
    pub(crate) const HANDLE_ADDR_MASK: u64 = crate::object::header::HANDLE_ADDR_MASK;
    /// The header's type word (the `Arc<TypeDesc>`'s pointer to its heap block).
    pub(crate) const TYPE_DESC_OFFSET: i32 = crate::object::header::KIND_WORD_OFFSET as i32;
    /// Field cell 0: a `FieldLoc::offset` is relative to this.
    pub(crate) const FIELDS_OFFSET: i32 = GcBlockHeader::DATA_OFFSET as i32;
    /// The block header's `type_tag` byte: block type, `gen_age`, remembered bit.
    pub(crate) const TYPE_TAG_OFFSET: i32 = crate::object::header::HEADER_TYPE_TAG_OFFSET as i32;
    /// `gen_age` = `(type_tag >> AGE_SHIFT) & AGE_MASK` (var-region blocks: objects, strings,
    /// closures alike).
    pub(crate) const AGE_SHIFT: i64 = 3;
    pub(crate) const AGE_MASK: i64 = 3;
    /// Set ⇔ the object is in the heap's remembered set (a minor re-scans it whole).
    pub(crate) const REMEMBERED_BIT: i64 = 1 << 5;

    // `Arc<T>` (sized `T`) is one pointer to its heap block — the word the header stores.
    const _: () = assert!(std::mem::size_of::<std::sync::Arc<u64>>() == 8);
    const _: () = assert!(std::mem::align_of::<crate::object::TypeDesc>() == std::mem::align_of::<u64>());

    /// Offset from the `type_desc` word's pointer to `TypeDesc::id` (a `u32`): the `Arc`'s
    /// counters, then the descriptor. Measured on a probe `Arc` of the same alignment rather than
    /// assumed, so a std layout change shows up here (and in the pinning test), not as a wrong load.
    pub(crate) fn type_id_offset() -> i32 {
        let probe = std::sync::Arc::new(0u64);
        // SAFETY: an `Arc<u64>` is exactly one pointer (asserted above); this reads it as an integer.
        let block: usize = unsafe { std::mem::transmute_copy::<std::sync::Arc<u64>, usize>(&probe) };
        let data = std::sync::Arc::as_ptr(&probe) as usize - block;
        (data + std::mem::offset_of!(crate::object::TypeDesc, id)) as i32
    }
}

// SAFETY: the header's fields are `Send + Sync` (the GC word is atomics, `type_desc` an `Arc`);
// the payload is only ever accessed through atomic cell views (`Cells`) and the meta word, which
// is written only while the object is exclusively owned (`&mut`, before publication) — so sharing
// `&ScriptObject` across threads is race-free and the `UnsafeCell`s are not a plain-memory
// mutation channel. What the meta word may own (`ObjExtras`) is `Send + Sync` (asserted above).
unsafe impl Send for ScriptObject {}
unsafe impl Sync for ScriptObject {}

impl std::fmt::Debug for ScriptObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptObject")
            .field("type", &self.type_desc.name)
            .field("type_args", &self.type_args())
            .field("native", self.native())
            .field("payload_len", &self.payload_len())
            .finish()
    }
}

impl ScriptObject {
    /// Release what the object owns — its type descriptor and, when its meta word holds one, the
    /// extras box — and return the bytes the box held outside the block (the drop glue credits
    /// them back). `ScriptObject` has no `Drop`: an object is only ever destroyed here, by the
    /// heap's drop glue (once per dead block) or by `OwnedObject`, and this reads the meta bit
    /// once on the sweep's hot path instead of once per step.
    ///
    /// # Safety
    /// `p` is a constructed object (type word installed) over its whole block, destroyed exactly
    /// once; nothing reads it afterwards.
    #[inline]
    pub(crate) unsafe fn destroy(p: *mut ScriptObject) -> u64 {
        // SAFETY: (all below) per the contract; the fields are in range of the fat pointer.
        unsafe {
            let mut freed = 0;
            if let Some(m) = (*p).meta_ptr() {
                if let Some(e) = ObjMeta(m.read_unaligned()).extras_ptr() {
                    freed = crate::gc::footprint::malloc_size(std::mem::size_of::<ObjExtras>());
                    drop(Box::from_raw(e));
                }
                m.write_unaligned(std::ptr::null());
            }
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*p).type_desc));
            freed
        }
    }

    /// The object over the block at `header` whose payload is `payload_size` bytes, as a fat raw
    /// pointer whose metadata is the payload length.
    ///
    /// # Safety
    /// `header` carries the provenance of the whole block (derived from the block's own
    /// allocation, never from a narrower reference) and the block's capacity covers
    /// `size_of_val` of the result (`DATA_OFFSET + payload_size` rounded up to 8 — every size
    /// class / detached block does). The caller installs the type word before reading through
    /// the pointer.
    #[inline(always)]
    pub(crate) unsafe fn from_block(header: NonNull<u8>, payload_size: usize) -> *mut ScriptObject {
        std::ptr::slice_from_raw_parts_mut(header.as_ptr(), payload_size) as *mut ScriptObject
    }

    /// Bytes of the meta word at the payload's end (0 or 8).
    #[inline(always)]
    fn meta_len(&self) -> usize {
        if self.gc.has_obj_meta() { META_BYTES } else { 0 }
    }

    /// The meta word's address (unaligned), if the object has one.
    #[inline]
    fn meta_ptr(&self) -> Option<*mut *const ()> {
        let m = self.meta_len();
        // SAFETY: the meta word is the payload's last `META_BYTES` bytes, in range of `&self`.
        (m != 0).then(|| unsafe {
            std::cell::UnsafeCell::raw_get(self.payload.as_ptr()).add(self.payload.len() - m).cast::<*const ()>()
        })
    }

    /// The meta word's value (`ObjMeta::NONE` when the object has none).
    #[inline]
    fn meta(&self) -> ObjMeta {
        // SAFETY: see `meta_ptr`; the word is written only under `&mut self`.
        self.meta_ptr().map_or(ObjMeta::NONE, |p| ObjMeta(unsafe { p.read_unaligned() }))
    }

    /// Rewrite the meta word.
    ///
    /// # Safety
    /// Construction only: the object is not published (nothing else reads it). Takes `&self`
    /// (the word is interior-mutable payload) so the heap can build an object in its block
    /// without a `&mut` over the block header, which a heap walk may read concurrently (M12).
    unsafe fn update_meta(&self, f: impl FnOnce(&mut ObjMeta)) {
        let p = self.meta_ptr().expect(
            "object has no meta word: allocate it with one (`OwnedObject` grows one on demand)",
        );
        // SAFETY: in range of `&self`, interior-mutable, and exclusive per the contract.
        unsafe {
            let mut m = ObjMeta(p.read_unaligned());
            f(&mut m);
            p.write_unaligned(m.0);
        }
    }

    /// Whether the object carries a meta word (a generic / native-backed instance).
    #[inline]
    pub fn has_meta(&self) -> bool {
        self.meta_len() != 0
    }

    /// Length of the field area in bytes (a boxed primitive's scalar width, a boxed struct's
    /// blob size, a class instance's `TypeDesc::object_bytes_len`) — the payload without the meta
    /// word.
    #[inline(always)]
    pub fn payload_len(&self) -> usize {
        self.payload.len() - self.meta_len()
    }

    /// Atomic loads and stores of the field cells.
    #[inline(always)]
    pub fn cells(&self) -> Cells<'_> {
        // SAFETY: the field area is the start of this value's own tail (in range of `&self`),
        // 8-aligned (the payload starts at the 8-aligned block's `DATA_OFFSET`),
        // `payload_len()` bytes, and interior-mutable; every access to it after construction is
        // an atomic cell access (JIT inline loads / stores are plain same-width accesses, the
        // relaxed atomics of x86-64 / AArch64).
        unsafe { Cells::from_raw(std::cell::UnsafeCell::raw_get(self.payload.as_ptr()), self.payload_len()) }
    }

    /// An atomic snapshot of the whole field area ([`Cells::copy_out`]).
    pub fn snapshot_fields(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.payload_len()];
        self.cells().copy_out(0, &mut out);
        out
    }

    /// The object's native backing (`None` for every ordinary user class).
    #[inline]
    pub fn native(&self) -> &NativeData {
        const NONE: &NativeData = &NativeData::None;
        // SAFETY: a tagged meta word owns a live `Box<ObjExtras>` that lives as long as the object.
        self.meta().extras_ptr().map_or(NONE, |e| unsafe { &(*e).native })
    }

    /// Per-instance generic type-arguments; empty for non-generic instances.
    #[inline]
    pub fn type_args(&self) -> &'static [String] {
        self.meta().type_args()
    }

    /// The type-arguments **handle** (the interned entry itself — M13: its per-argument packed
    /// kinds are computed once at intern time). Empty for non-generic instances.
    #[inline]
    pub fn type_args_handle(&self) -> crate::object::TypeArgs {
        self.meta().type_args_handle()
    }

    /// Install the native backing, allocating the cold extras box on first use. Construction
    /// only (`&mut self`): a published object is shared without a lock, so its extras never
    /// change — build the object completely before it is published. **Panics** when a real
    /// backing is installed on an object without a meta word (allocate it with one;
    /// [`OwnedObject::set_native`] grows one).
    #[inline]
    pub fn set_native(&mut self, native: NativeData) {
        if matches!(native, NativeData::None) && self.meta().extras_ptr().is_none() {
            return; // nothing to record — stay box-free
        }
        // SAFETY: `&mut self` — not shared; `extras_or_alloc` hands out the owned box.
        unsafe { self.update_meta(|m| (*m.extras_or_alloc()).native = native) };
    }

    /// Install the per-instance type-arguments handle (an interned instantiation; empty
    /// for a non-generic `new`). Never allocates unless the object is native-backed.
    /// Construction only, like [`Self::set_native`] — and like it, a non-empty list needs the
    /// meta word.
    #[inline]
    pub fn set_type_args(&mut self, type_args: TypeArgs) {
        // SAFETY: `&mut self` — not shared.
        unsafe { self.set_type_args_unpublished(type_args) }
    }

    /// [`Self::set_type_args`] through `&self`.
    ///
    /// # Safety
    /// The object is not published yet (see `update_meta`).
    #[inline]
    unsafe fn set_type_args_unpublished(&self, type_args: TypeArgs) {
        if type_args.is_empty() && !self.has_meta() {
            return;
        }
        // SAFETY: per the contract.
        unsafe { self.update_meta(|m| m.set_type_args(type_args)) };
    }

    /// `ObjNew`'s generic step, shared by the interpreter (heap and stack objects) and the
    /// JIT: record the instantiation's type-arguments, then rewrite every type-parameter
    /// field whose argument is a primitive value type to that type's zero
    /// (fix-generic-typeparam-field-zero — see [`generic_field_zero_overrides`]). One
    /// implementation, so the engines and `Z42_STACKALLOC` cannot disagree on
    /// `GBox<int>().V`. No-op for an empty list.
    pub fn install_type_args(&mut self, type_args: TypeArgs) {
        // SAFETY: `&mut self` — not shared.
        unsafe { self.install_type_args_unpublished(type_args) }
    }

    /// [`Self::install_type_args`] through `&self` — the heap builds an object in its block this
    /// way, without a `&mut` over the block header.
    ///
    /// # Safety
    /// The object is not published yet: nothing else reads its payload.
    pub(crate) unsafe fn install_type_args_unpublished(&self, type_args: TypeArgs) {
        if type_args.is_empty() {
            return;
        }
        // SAFETY: per the contract.
        unsafe { self.set_type_args_unpublished(type_args) };
        for (slot, zero) in generic_field_zero_overrides(&self.type_desc, type_args.as_slice()) {
            self.set_field_value(slot, &zero);
        }
    }

    /// What this object holds from the allocator **outside** its block: the native extras box
    /// when present, rounded to the allocator's size class. Type-arguments are interned per
    /// instantiation and cost nothing here. The GC charges it to the heap footprint when the
    /// object is born and its drop glue credits it back when the block dies.
    pub fn extras_bytes(&self) -> u64 {
        if self.meta().extras_ptr().is_some() {
            crate::gc::footprint::malloc_size(std::mem::size_of::<ObjExtras>())
        } else {
            0
        }
    }

    /// unify Phase 2 R3（装箱统一）：若本对象是**整数基元装箱盒**（`type_desc` 是整数 wrapper、
    /// 标量 LE 字节存载荷，见 `corelib::convert::box_prim_to_heap`），读回其 i64 标量；
    /// 否则（多字段 struct 装箱 / 非整数 wrapper）返 `None`。按 wrapper 宽度 + 有无符号从
    /// 载荷前 `width` 字节还原（signed narrow → 符号扩展，unsigned → 零扩展）。
    /// 让装箱整数盒与 struct 装箱盒共用 `Value::BoxedStruct`，同时保留整数的透明拆箱语义。
    /// make-enum-distinct-type 1.5: an **enum box** is the same shape (i64 LE bytes,
    /// `TypeDesc` = the enum itself instead of a `Std.*` wrapper), so it unboxes here
    /// too — that is what makes `(Color)o` / `(long)o` transparent for every caller of
    /// this method (cast, compare, …) without each of them growing an enum arm.
    pub fn boxed_prim_i64(&self) -> Option<i64> {
        let (width, signed) = if self.type_desc.class_flags
            & crate::metadata::ir::CLASS_FLAG_ENUM
            != 0
        {
            (8usize, true)
        } else {
            crate::metadata::index::well_known_names::int_wrapper_scalar_spec(&self.type_desc.name)?
        };
        if self.payload_len() < width {
            return None;
        }
        // One relaxed load of the scalar cell (the scalar is stored little-endian at offset 0 by
        // `box_prim_to_heap`; the VM's targets are all little-endian).
        let mut v = self.cells().load_bits(0, width)? as i64;
        if signed && width < 8 {
            let shift = (8 - width) * 8;
            v = (v << shift) >> shift; // 符号扩展窄整数
        }
        Some(v)
    }

    /// make-enum-distinct-type 1.5: if this object is an **enum box**, the declared
    /// member name for the value it carries — C# `Enum.ToString()`; `None` for every
    /// other box, so callers keep their existing behaviour. Convenience for code that
    /// already holds the object; the real lookup is [`TypeDesc::enum_member_name`].
    pub fn boxed_enum_name(&self) -> Option<String> {
        self.type_desc.enum_member_name(self.boxed_prim_i64()?)
    }
}

impl crate::object::ObjRef {
    /// The object's `type_desc` (set at construction, never changed). Used by hot-path IC
    /// scans (VCallIC, FieldIC, IsInstance) and the GC mark traversal. Returns the inner
    /// `&TypeDesc` (one fewer deref at the call site than `&Arc<TypeDesc>`). Reads through the
    /// thin header (`ObjRef::header`): no payload length, no fat reference.
    #[inline]
    pub fn type_desc(&self) -> &TypeDesc {
        &self.header().type_desc
    }

    /// Lockless read of the object's `type_desc` as `&Arc<TypeDesc>`.
    /// Use this only when the caller needs to clone the Arc for
    /// ownership transfer (e.g. building a fallback TypeDesc, exception
    /// stack frames). Most callers want [`type_desc`] (returns plain
    /// `&TypeDesc`) which saves one deref.
    #[inline]
    pub fn type_desc_arc(&self) -> &Arc<TypeDesc> {
        &self.header().type_desc
    }

    /// Lockless read of the object's `type_args` (generic type arguments at construction).
    /// Same write-once invariant as `type_desc`: set by `ObjNew` before the reference
    /// escapes, never mutated after; the slice itself is an interned `'static` entry. Reads the
    /// meta word at the payload's end, so it goes through the fat reference.
    #[inline]
    pub fn type_args(&self) -> &'static [String] {
        self.borrow().type_args()
    }
}

// ── Value ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "object_tests.rs"]
mod object_tests;
