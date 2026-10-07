//! NativeData / ScriptObject + GcRef<ScriptObject> 访问。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
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
// Replaces the old `ObjectData`. Every class instance is represented as a
// `ScriptObject`, which combines:
//   1. A type descriptor pointer (Arc<TypeDesc>) — the class identity
//   2. A flat slot array (Vec<Value>)            — instance fields by index
//   3. Optional native backing (NativeData)      — for built-in types

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

/// Heap-allocated managed object with reference semantics (CoreCLR Object equivalent).
#[derive(Debug)]
pub struct ScriptObject {
    /// Type descriptor shared across all instances of this class.
    pub type_desc: Arc<TypeDesc>,
    /// The object's field payload — primitive leaves, 8 B reference words, type-parameter
    /// cells and the 16 B reference side table — in ONE allocation (see [`ObjStorage`]).
    ///
    /// Every primitive leaf (incl. inline-struct interior primitive leaves) lives at its
    /// composed byte offset in `storage.bytes()`, and so does every direct reference field,
    /// as an 8 B self-describing reference word (`ref_word`, `ObjectLayout::ref_cells`).
    /// A type-parameter field's tag word sits at its own offset too, its payload word past
    /// the compiler's layout (`tparam_cell`, `ObjectLayout::tparam_cells`). Only
    /// inline-struct interior reference leaves (and the references of a synthesized layout)
    /// are 16 B `Value`s in `storage.refs()`, ordered by `ObjectLayout::ref_offsets`. Field
    /// access goes through the methods in `object_fields.rs`.
    pub storage: ObjStorage,
    /// The two cold per-instance side-fields — generic type-arguments and native backing —
    /// in one 8 B word (see [`ObjMeta`]): a generic instance points at its instantiation's
    /// interned [`TypeArgs`] entry (shared, nothing allocated per object); only a
    /// native-backed object pays for a box.
    meta: ObjMeta,
}

impl ScriptObject {
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

    /// Install the native backing, allocating the cold extras box on first use.
    #[inline]
    pub fn set_native(&mut self, native: NativeData) {
        if matches!(native, NativeData::None) && self.meta.extras_ptr().is_none() {
            return; // nothing to record — stay box-free
        }
        self.meta.extras_or_alloc().native = native;
    }

    /// Install the per-instance type-arguments handle (an interned instantiation; empty
    /// for a non-generic `new`). Never allocates unless the object is native-backed.
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

    /// What this object holds from the allocator **outside** its GC slot: the field payload
    /// block and, when present, the native extras box — each rounded to the allocator's
    /// size class. Type-arguments are interned per instantiation and cost nothing here.
    /// The GC charges it to the heap footprint when the object is born and credits it when
    /// the dead object's slot is reused.
    pub fn heap_payload_bytes(&self) -> u64 {
        use crate::gc::footprint::malloc_size;
        let mut n = malloc_size(self.storage.alloc_bytes());
        if self.meta.extras_ptr().is_some() {
            n += malloc_size(std::mem::size_of::<ObjExtras>());
        }
        n
    }

    /// Construct with no extras — the common case.
    #[inline]
    pub fn new(type_desc: std::sync::Arc<TypeDesc>, storage: ObjStorage) -> Self {
        Self { type_desc, storage, meta: ObjMeta::NONE }
    }

    /// Construct with a native backing (built-in boxes).
    #[inline]
    pub fn with_native(
        type_desc: std::sync::Arc<TypeDesc>, storage: ObjStorage, native: NativeData,
    ) -> Self {
        let mut o = Self::new(type_desc, storage);
        o.set_native(native);
        o
    }

    /// shrink-object-footprint P2: the byte-packed primitive leaves.
    #[inline] pub fn bytes(&self) -> &[u8] { self.storage.bytes() }
    /// Mutable view of the primitive leaves.
    #[inline] pub fn bytes_mut(&mut self) -> &mut [u8] { self.storage.bytes_mut() }
    /// The reference leaves, in composed reference-bitmap order.
    #[inline] pub fn refs(&self) -> &[Value] { self.storage.refs() }
    /// Mutable view of the reference leaves (GC scan / field store).
    /// Reference leaves **without the SATB barrier** — see [`ObjStorage::refs_mut_raw`].
    #[inline] pub fn refs_mut_raw(&mut self) -> &mut [Value] { self.storage.refs_mut_raw() }

    /// unify Phase 2 R3（装箱统一）：若本对象是**整数基元装箱盒**（`type_desc` 是整数 wrapper、
    /// 标量 LE 字节存 `struct_bytes`，见 `corelib::convert::box_prim_to_heap`），读回其 i64 标量；
    /// 否则（多字段 struct 装箱 / 非整数 wrapper）返 `None`。按 wrapper 宽度 + 有无符号从
    /// `struct_bytes` 前 `width` 字节还原（signed narrow → 符号扩展，unsigned → 零扩展）。
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
        if self.bytes().len() < width {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..width].copy_from_slice(&self.bytes()[..width]);
        let mut v = i64::from_le_bytes(buf);
        if signed && width < 8 {
            let shift = (8 - width) * 8;
            v = (v << shift) >> shift; // 符号扩展窄整数
        }
        Some(v)
    }

    /// make-enum-distinct-type 1.5: if this object is an **enum box**, the declared
    /// member name for the value it carries — C# `Enum.ToString()`; `None` for every
    /// other box, so callers keep their existing behaviour. Convenience for code that
    /// already holds the `borrow()` guard; the real lookup is
    /// [`TypeDesc::enum_member_name`], which lock-free callers use directly.
    pub fn boxed_enum_name(&self) -> Option<String> {
        self.type_desc.enum_member_name(self.boxed_prim_i64()?)
    }
}

impl crate::gc::GcRef<ScriptObject> {
    /// **extract-typedesc-from-mutex (2026-05-31)**: lockless read of
    /// the object's `type_desc`. type_desc is set by `alloc_object` and
    /// never mutated for the object's lifetime — there's no concurrent
    /// writer, so bypassing the per-entry Mutex is sound. Used by
    /// hot-path IC scans (VCallIC, FieldIC, IsInstance) and the GC mark
    /// traversal.
    ///
    /// Returns a `&TypeDesc` borrowed for the GcRef's lifetime. The
    /// Arc itself stays alive through the entry's storage; the borrow
    /// is to the inner TypeDesc directly (one fewer deref at the call
    /// site than returning `&Arc<TypeDesc>`).
    #[inline]
    pub fn type_desc(&self) -> &TypeDesc {
        // SAFETY: type_desc is write-once-at-alloc. Verified 0 mutation
        // sites in the runtime via `grep -rn '.type_desc *=' src/`.
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_desc }
    }

    /// Lockless read of the object's `type_desc` as `&Arc<TypeDesc>`.
    /// Use this only when the caller needs to clone the Arc for
    /// ownership transfer (e.g. building a fallback TypeDesc, exception
    /// stack frames). Most callers want [`type_desc`] (returns plain
    /// `&TypeDesc`) which saves one deref.
    #[inline]
    pub fn type_desc_arc(&self) -> &Arc<TypeDesc> {
        // SAFETY: see type_desc() — write-once invariant.
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_desc }
    }

    /// Lockless read of the object's `type_args` (generic type arguments at construction).
    /// Same write-once invariant as `type_desc`: set by `ObjNew` before the reference
    /// escapes, never mutated after; the slice itself is an interned `'static` entry.
    #[inline]
    pub fn type_args(&self) -> &[String] {
        // SAFETY: type_args is write-once-at-alloc; see type_desc().
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_args() }
    }
}

// ── Value ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "object_tests.rs"]
mod object_tests;
