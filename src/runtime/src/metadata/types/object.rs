//! NativeData / ScriptObject（对象头）+ ObjRef 访问。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// Native backing data for built-in classes.
///
/// Used by `ScriptObject` to hold VM-managed state that should not be
/// directly accessible as a z42 field (i.e. not visible in `slots`).
#[derive(Debug, Clone)]
pub enum NativeData {
    /// No native backing — ordinary user-defined class.
    None,
    /// 2026-05-04 expose-weak-ref-builtin (D-1a)：包装 GC 弱引用句柄。
    /// 由 `__obj_make_weak` builtin 创建；`__obj_upgrade_weak` 升格回原对象。
    /// 用户视角是 `Std.WeakHandle` 类（无字段）。
    WeakRef(crate::gc::WeakRef),
    /// 2026-06-08 add-reflection-mvp：`Std.Type` 对象携带的真实类型句柄。
    /// 由 `__obj_get_type` 对 `Value::Object` 创建（存对象 `type_desc` 的
    /// `Arc<TypeDesc>`）；反射 builtins（`__type_fields` / `__type_methods` /
    /// `__type_base` / `__type_generic_args`）据此枚举成员。基础类型/数组的
    /// synthetic Type 无此句柄（`NativeData::None`），成员查询退化为空。
    TypeHandle(Arc<TypeDesc>),
    /// 2026-07-30 add-load-context-model：`Std.Runtime.LoadContext` 对象携带的
    /// 上下文句柄（root = `ContextId::ROOT`）。`__lctx_*` builtins 据此查
    /// `VmCore.context_registry`。
    LoadContextHandle(crate::metadata::context::ContextId),
    /// 2026-07-30 add-load-context-model：`Std.Reflection.Assembly` 对象携带的
    /// 程序集句柄（zpkg 运行时投影）。`__asm_*` builtins 据此查注册表。
    AssemblyHandle(crate::metadata::context::AssemblyId),
    /// 2026-09-14 store-sync-values-in-heap：`Std.Threading.Mutex/RwLock/Channel` 的同步底座。
    /// **不含任何 GC 值**（值是 z42 字段）；句柄对象的槽位被复用时随旧条目 drop。
    Monitor(std::sync::Arc<crate::corelib::monitor::Monitor>),
    // 2026-04-26 script-first-stringbuilder: removed `StringBuilder(String)` —
    // `Std.Text.StringBuilder` is now a pure z42 script. Variant slot kept open
    // for future native-backed types (Stream / FileHandle / etc.).
}

// ── ScriptObject — unified managed object ───────────────────────────────────
//
// Every class instance (and every boxed value struct / primitive) is a `ScriptObject` header —
// the type descriptor (class identity) and one meta word (generic arguments, native backing) —
// followed in the same GC block by its field cells (see the type docs below).

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

/// One 8 B word for an object's cold metadata (`type_args` + native backing).
///
/// - null — non-generic, no native backing (the overwhelming majority);
/// - an even pointer — the interned [`TypeArgs`] entry of a generic instance (no allocation:
///   every instance of one instantiation shares the entry);
/// - an odd pointer — a [`Box<ObjExtras>`] tagged with the low bit (native-backed objects).
///
/// Both pointees are ≥ 8-aligned, so the low bit is free for the tag.
struct ObjMeta(*const ());

// SAFETY: the word is either null, a pointer to an immutable leaked `'static` `TypeArgsInfo`,
// or an owned `Box<ObjExtras>` whose contents are `Send + Sync` (asserted above). It is only
// mutated through `&mut self`.
unsafe impl Send for ObjMeta {}
// SAFETY: see the `Send` impl — shared access only reads.
unsafe impl Sync for ObjMeta {}

impl ObjMeta {
    const EXTRAS_TAG: usize = 1;
    const NONE: Self = ObjMeta(std::ptr::null());

    #[inline]
    fn extras_ptr(&self) -> Option<*mut ObjExtras> {
        if self.0.addr() & Self::EXTRAS_TAG != 0 {
            Some(self.0.map_addr(|a| a & !Self::EXTRAS_TAG) as *mut ObjExtras)
        } else {
            None
        }
    }

    #[inline]
    fn extras(&self) -> Option<&ObjExtras> {
        // SAFETY: a tagged word is an owned, live `Box<ObjExtras>` (see `extras_or_alloc`).
        self.extras_ptr().map(|p| unsafe { &*p })
    }

    #[inline]
    fn type_args(&self) -> &'static [String] {
        if self.0.is_null() {
            return &[];
        }
        match self.extras() {
            Some(e) => e.type_args.as_slice(),
            // SAFETY: an untagged non-null word came from `TypeArgs::as_raw`.
            None => unsafe { TypeArgs::from_raw(self.0 as *const TypeArgsInfo) }.as_slice(),
        }
    }

    #[inline]
    fn native(&self) -> &NativeData {
        const NONE: &NativeData = &NativeData::None;
        self.extras().map_or(NONE, |e| &e.native)
    }

    /// The extras box, allocating it (and carrying any type-args handle over) on first use.
    fn extras_or_alloc(&mut self) -> &mut ObjExtras {
        let p = match self.extras_ptr() {
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
        };
        // SAFETY: `p` is the owned `Box<ObjExtras>` this word holds; `&mut self` gives
        // exclusive access.
        unsafe { &mut *p }
    }

    #[inline]
    fn set_type_args(&mut self, ta: TypeArgs) {
        match self.extras_ptr() {
            // SAFETY: owned box, exclusive via `&mut self`.
            Some(p) => unsafe { (*p).type_args = ta },
            None if ta.is_empty() => self.0 = std::ptr::null(),
            None => self.0 = ta.as_raw() as *const (),
        }
    }
}

impl Drop for ObjMeta {
    fn drop(&mut self) {
        if let Some(p) = self.extras_ptr() {
            // SAFETY: a tagged word owns the `Box<ObjExtras>` it was made from.
            drop(unsafe { Box::from_raw(p) });
        }
    }
}

impl std::fmt::Debug for ObjMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjMeta")
            .field("type_args", &self.type_args())
            .field("native", self.native())
            .finish()
    }
}

/// A managed object's **header** (object model R6) — the first 16 bytes of its storage, with
/// the field cells immediately after it:
///
/// ```text
///   GcBlockHeader (16 B) │ ScriptObject: type_desc (8 B) │ meta (8 B) │ field cells …
///                         ↑ &ScriptObject                             ↑ cells(): payload_len() bytes
/// ```
///
/// A heap object is one `BlockType::Object` block of the variable-length GC region: block
/// header, this header, fields — one allocation, freed (and its `Arc<TypeDesc>` / extras box
/// dropped by the heap's drop glue) when the block is tombstoned. An object outside the heap (a
/// stack-allocated object, one under construction) lives in an [`OwnedObject`], a detached
/// block of the same shape — so every `&ScriptObject` finds its block header 16 bytes before it
/// and its fields 16 bytes after it, and the field area's length is the block header's payload
/// size minus this header.
///
/// `ScriptObject` is therefore never a free-standing value: it is only ever reached by
/// reference, through an [`ObjRef`](crate::gc::ObjRef) or an `OwnedObject`.
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
    /// Type descriptor shared across all instances of this class. Set before the object is
    /// published, never changed.
    pub type_desc: Arc<TypeDesc>,
    /// The two cold per-instance side-fields — generic type-arguments and native backing —
    /// in one 8 B word (see [`ObjMeta`]): a generic instance points at its instantiation's
    /// interned [`TypeArgs`] entry (shared, nothing allocated per object); only a
    /// native-backed object pays for a box.
    meta: ObjMeta,
    /// The trailing field area, zero-sized here. An `UnsafeCell` so `&ScriptObject` is not
    /// `Freeze`: the field cells behind the header are written through `&self` (atomic cell
    /// stores), which a `readonly` / `noalias` shared reference would forbid.
    fields: std::cell::UnsafeCell<[u64; 0]>,
}

/// Bytes of the object header in front of the field cells (object model R6): two pointer-sized
/// words — 16 on 64-bit targets, 8 on 32-bit ones (wasm32 / armv7). A multiple of 8 either way,
/// so the field cells start 8-aligned behind the 8-aligned block payload.
pub const OBJ_HEADER_BYTES: usize = std::mem::size_of::<ScriptObject>();
#[cfg(target_pointer_width = "64")]
const _: () = assert!(OBJ_HEADER_BYTES == 16);
const _: () = assert!(OBJ_HEADER_BYTES % 8 == 0);
const _: () = assert!(std::mem::align_of::<ScriptObject>() == 8);

// SAFETY: the header's fields are `Send + Sync` (`ObjMeta`'s impls say why); the field area is
// only ever accessed through atomic cell views (`Cells`), so sharing `&ScriptObject` across
// threads is race-free — the `UnsafeCell` marker is not a plain-memory mutation channel.
unsafe impl Send for ScriptObject {}
unsafe impl Sync for ScriptObject {}

impl std::fmt::Debug for ScriptObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptObject")
            .field("type", &self.type_desc.name)
            .field("meta", &self.meta)
            .field("payload_len", &self.payload_len())
            .finish()
    }
}

impl ScriptObject {
    /// The header of a fresh object of `type_desc` (no extras) — written into a block by the
    /// allocators (`OwnedObject`, `ArcMagrGC`), never used as a free-standing value.
    #[inline]
    pub(crate) fn header(type_desc: Arc<TypeDesc>) -> Self {
        Self { type_desc, meta: ObjMeta::NONE, fields: std::cell::UnsafeCell::new([]) }
    }

    /// The block header 16 bytes in front of this object header (a heap block's, or an
    /// `OwnedObject`'s detached block's).
    #[inline(always)]
    fn block_header(&self) -> &crate::gc::var_region::GcBlockHeader {
        // SAFETY: a `ScriptObject` only exists at the payload start of a block (see the type
        // docs), so the 16 bytes before it are that block's header.
        unsafe {
            &*(self as *const Self)
                .cast::<u8>()
                .sub(crate::gc::var_region::GcBlockHeader::DATA_OFFSET)
                .cast::<crate::gc::var_region::GcBlockHeader>()
        }
    }

    /// Payload length in bytes — the field area behind the header (a boxed primitive's scalar
    /// width, a boxed struct's blob size, a class instance's `TypeDesc::object_bytes_len`).
    #[inline(always)]
    pub fn payload_len(&self) -> usize {
        self.block_header().size() - OBJ_HEADER_BYTES
    }

    /// Atomic loads and stores of the field cells.
    #[inline(always)]
    pub fn cells(&self) -> Cells<'_> {
        // SAFETY: the field area starts right after the header, 8-aligned (block payloads are
        // 8-aligned and the header is 16 bytes), `payload_len()` bytes long and owned by this
        // object's block; every access to it after construction is an atomic cell access (JIT
        // inline loads / stores are plain same-width accesses, the relaxed atomics of x86-64 /
        // AArch64).
        unsafe { Cells::from_raw(self.fields.get().cast::<u8>(), self.payload_len()) }
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
        self.meta.native()
    }

    /// Per-instance generic type-arguments; empty for non-generic instances.
    #[inline]
    pub fn type_args(&self) -> &[String] {
        self.meta.type_args()
    }

    /// Install the native backing, allocating the cold extras box on first use. Construction
    /// only (`&mut self`): a published object is shared without a lock, so its extras never
    /// change — build the object completely before it is published.
    #[inline]
    pub fn set_native(&mut self, native: NativeData) {
        if matches!(native, NativeData::None) && self.meta.extras_ptr().is_none() {
            return; // nothing to record — stay box-free
        }
        self.meta.extras_or_alloc().native = native;
    }

    /// Install the per-instance type-arguments handle (an interned instantiation; empty
    /// for a non-generic `new`). Never allocates unless the object is native-backed.
    /// Construction only, like [`Self::set_native`].
    #[inline]
    pub fn set_type_args(&mut self, type_args: TypeArgs) {
        self.meta.set_type_args(type_args);
    }

    /// `ObjNew`'s generic step, shared by the interpreter (heap and stack objects) and the
    /// JIT: record the instantiation's type-arguments, then rewrite every type-parameter
    /// field whose argument is a primitive value type to that type's zero
    /// (fix-generic-typeparam-field-zero — see [`generic_field_zero_overrides`]). One
    /// implementation, so the engines and `Z42_STACKALLOC` cannot disagree on
    /// `GBox<int>().V`. No-op for an empty list.
    pub fn install_type_args(&mut self, type_args: TypeArgs) {
        if type_args.is_empty() {
            return;
        }
        self.set_type_args(type_args);
        for (slot, zero) in generic_field_zero_overrides(&self.type_desc, type_args.as_slice()) {
            self.set_field_value(slot, &zero);
        }
    }

    /// What this object holds from the allocator **outside** its block: the native extras box
    /// when present, rounded to the allocator's size class. Type-arguments are interned per
    /// instantiation and cost nothing here. The GC charges it to the heap footprint when the
    /// object is born and its drop glue credits it back when the block dies.
    pub fn extras_bytes(&self) -> u64 {
        if self.meta.extras_ptr().is_some() {
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
            & crate::metadata::bytecode::CLASS_FLAG_ENUM
            != 0
        {
            (8usize, true)
        } else {
            crate::metadata::well_known_names::int_wrapper_scalar_spec(&self.type_desc.name)?
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

impl crate::gc::ObjRef {
    /// The object's `type_desc` (set at construction, never changed). Used by hot-path IC
    /// scans (VCallIC, FieldIC, IsInstance) and the GC mark traversal. Returns the inner
    /// `&TypeDesc` (one fewer deref at the call site than `&Arc<TypeDesc>`).
    #[inline]
    pub fn type_desc(&self) -> &TypeDesc {
        &self.borrow().type_desc
    }

    /// Lockless read of the object's `type_desc` as `&Arc<TypeDesc>`.
    /// Use this only when the caller needs to clone the Arc for
    /// ownership transfer (e.g. building a fallback TypeDesc, exception
    /// stack frames). Most callers want [`type_desc`] (returns plain
    /// `&TypeDesc`) which saves one deref.
    #[inline]
    pub fn type_desc_arc(&self) -> &Arc<TypeDesc> {
        &self.borrow().type_desc
    }

    /// Lockless read of the object's `type_args` (generic type arguments at construction).
    /// Same write-once invariant as `type_desc`: set by `ObjNew` before the reference
    /// escapes, never mutated after; the slice itself is an interned `'static` entry.
    #[inline]
    pub fn type_args(&self) -> &[String] {
        self.borrow().type_args()
    }
}

// ── Value ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "object_tests.rs"]
mod object_tests;
