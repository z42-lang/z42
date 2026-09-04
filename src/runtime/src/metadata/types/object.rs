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

/// shrink-object-footprint P3: the cold per-instance side-fields of a
/// [`ScriptObject`]. Allocated only when at least one of them is non-empty.
#[derive(Debug, Default)]
struct ObjExtras {
    /// Native backing for built-in types (WeakRef / Type / LoadContext / Assembly).
    native: NativeData,
    /// 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): per-instance
    /// generic type-arguments. For `new Foo<int, string>()` this is
    /// `["int", "string"]`. Index aligns with `type_desc.type_params`.
    /// Read by `DefaultOf` and runtime type-args queries.
    type_args: Box<[String]>,
}

impl Default for NativeData {
    fn default() -> Self { NativeData::None }
}

/// Heap-allocated managed object with reference semantics (CoreCLR Object equivalent).
#[derive(Debug)]
pub struct ScriptObject {
    /// Type descriptor shared across all instances of this class.
    pub type_desc: Arc<TypeDesc>,
    /// shrink-object-footprint P2: the object's field payload — the **byte-packed**
    /// primitive leaves and the **reference leaves**, in ONE allocation
    /// (see [`ObjStorage`]).
    ///
    /// Every primitive leaf of every direct field (incl. inline-struct interior
    /// primitive leaves) lives at its composed byte offset (`ObjectLayout::field_access`
    /// / `field_offsets`) in `storage.bytes()`; every reference leaf lives in
    /// `storage.refs()`, ordered by the composed reference bitmap
    /// (`ObjectLayout::ref_offsets` / `FieldAccess::ref_slot`). GC scans the reference
    /// side directly (`visitor(&Value)`); a write to one is a plain `Value`-slot store
    /// routed through `write_barrier_field`.
    ///
    /// Was two separate boxed slices (`bytes: Box<[u8]>` + `refs: Box<[Value]>`) =
    /// two mallocs and 32 bytes of fat pointer per object.
    pub storage: ObjStorage,
    /// shrink-object-footprint P3: the two **cold** per-instance side-fields —
    /// native backing and generic type-arguments — behind one optional box.
    ///
    /// Both are empty for the overwhelming majority of objects: `NativeData::None`
    /// for every ordinary user class (only WeakRef / Type / LoadContext / Assembly
    /// carry a handle), and no type-args for every non-generic instantiation.
    /// Inline they cost 16 + 16 = **32 bytes on every object**; as
    /// `Option<Box<ObjExtras>>` they cost 8, and only an object that actually has
    /// one of them pays for the box.
    extras: Option<Box<ObjExtras>>,
}

impl ScriptObject {
    /// shrink-object-footprint P3: the object's native backing (`None` when it has
    /// no extras box, which is the common case).
    #[inline]
    pub fn native(&self) -> &NativeData {
        const NONE: &NativeData = &NativeData::None;
        self.extras.as_ref().map_or(NONE, |e| &e.native)
    }

    /// Per-instance generic type-arguments; empty for non-generic instances.
    #[inline]
    pub fn type_args(&self) -> &[String] {
        self.extras.as_ref().map_or(&[], |e| &e.type_args)
    }

    /// Install the native backing, allocating the cold extras box on first use.
    #[inline]
    pub fn set_native(&mut self, native: NativeData) {
        if matches!(native, NativeData::None) && self.extras.is_none() {
            return; // nothing to record — stay box-free
        }
        self.extras.get_or_insert_with(Default::default).native = native;
    }

    /// Install per-instance type-arguments (no-op for an empty list on a
    /// box-free object, so a non-generic `new` never allocates the box).
    #[inline]
    pub fn set_type_args(&mut self, type_args: Box<[String]>) {
        if type_args.is_empty() && self.extras.is_none() {
            return;
        }
        self.extras.get_or_insert_with(Default::default).type_args = type_args;
    }

    /// Construct with no extras — the common case.
    #[inline]
    pub fn new(type_desc: std::sync::Arc<TypeDesc>, storage: ObjStorage) -> Self {
        Self { type_desc, storage, extras: None }
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
    #[inline] pub fn refs_mut(&mut self) -> &mut [Value] { self.storage.refs_mut() }

    /// unify Phase 2 R3（装箱统一）：若本对象是**整数基元装箱盒**（`type_desc` 是整数 wrapper、
    /// 标量 LE 字节存 `struct_bytes`，见 `corelib::convert::box_prim_to_heap`），读回其 i64 标量；
    /// 否则（多字段 struct 装箱 / 非整数 wrapper）返 `None`。按 wrapper 宽度 + 有无符号从
    /// `struct_bytes` 前 `width` 字节还原（signed narrow → 符号扩展，unsigned → 零扩展）。
    /// 让装箱整数盒与 struct 装箱盒共用 `Value::BoxedStruct`，同时保留整数的透明拆箱语义。
    pub fn boxed_prim_i64(&self) -> Option<i64> {
        let (width, signed) =
            crate::metadata::well_known_names::int_wrapper_scalar_spec(&self.type_desc.name)?;
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

    /// unify-object-byte-layout (PR-2): the resolved `FieldAccess` for a direct field
    /// `slot` (see `TypeDesc::field_index`). Reads the type's composed object layout;
    /// falls back to on-the-fly synthesis for a layout-less type (rare — synthetic /
    /// Rust-constructed). `FieldAccess` is `Copy`, so no borrow of the layout escapes.
    #[inline]
    fn field_access_of(&self, slot: usize) -> Option<FieldAccess> {
        if let Some(col) = self.type_desc.composed_object_layout() {
            return col.field_access.get(slot).copied();
        }
        if self.type_desc.fields.is_empty() { return None; }
        synthesize_object_layout(&self.type_desc.fields).field_access.get(slot).copied()
    }

    /// unify-object-byte-layout (PR-2): read direct field `slot` as a `Value`.
    /// Primitive → `decode_prim` off `bytes`; reference → the `refs` side-table cell.
    /// `Null` for an out-of-range slot or a struct-typed root (accessed via
    /// `StructFieldGetPrim`, never `FieldGet`). Replaces `self.slots[slot].clone()`.
    #[inline]
    pub fn field_value(&self, slot: usize) -> Value {
        let fa = match self.field_access_of(slot) { Some(f) => f, None => return Value::Null };
        if fa.ref_slot >= 0 {
            return self.refs().get(fa.ref_slot as usize).cloned().unwrap_or(Value::Null);
        }
        // PR-3 chunk 2b: an inlined direct object/array reference (`ref_slot == -1` but a
        // reference tag) — read the 8B tagged pointer straight from `bytes` (0 = `Null`).
        if fa.tag == TAG_OBJECT || fa.tag == TAG_ARRAY {
            return read_inline_ref(&self.bytes(), fa.offset as usize, fa.tag == TAG_ARRAY);
        }
        if fa.tag == TAG_UNKNOWN {
            return Value::Null; // struct-typed root — not a FieldGet target
        }
        decode_prim(&self.bytes(), fa.offset as usize, fa.width as usize, fa.tag)
            .unwrap_or(Value::Null)
    }

    /// post-layout JIT perf (P5-B): if `name` is a direct **inline primitive**
    /// field — a scalar packed in `bytes`, NOT a `refs` side-table reference, a
    /// byte-inlined object/array pointer, a struct-typed root, or a string — return
    /// `(bytes base ptr, byte offset, width, tag)`. The JIT hoists this once per
    /// never-reassigned object and emits a native width-aware byte load/store
    /// (mirroring `decode_prim`/`encode_prim`) instead of calling `jit_field_get`/
    /// `jit_field_set`. `None` (→ keep the helper) for anything else, so reference
    /// writes still fire the GC `write_barrier_field` and struct/string/polymorphic
    /// access keeps its full semantics. The returned pointer is valid for the frame:
    /// non-moving GC + fixed `bytes` allocation + caller holds the object live.
    #[inline]
    pub fn inline_prim_field(&self, name: &str) -> Option<(*const u8, u32, u32, u8)> {
        let slot = *self.type_desc.field_index.get(name)?;
        let fa = self.field_access_of(slot)?;
        if fa.ref_slot >= 0 { return None; } // reference in `refs` side-table
        match fa.tag {
            // byte-inlined obj/array ref, struct root, or string → not a scalar prim
            TAG_OBJECT | TAG_ARRAY | TAG_UNKNOWN | TAG_STR => return None,
            _ => {}
        }
        Some((self.bytes().as_ptr(), fa.offset, fa.width, fa.tag))
    }

    /// post-layout JIT perf (T1-B): if `name` is a direct **byte-inlined reference**
    /// field — a class-instance (`STRUCT_LEAF_GCREF` → `TAG_OBJECT`) or array
    /// (`STRUCT_LEAF_GCREF_ARRAY` → `TAG_ARRAY`) whose 8B tagged pointer lives in
    /// `bytes` (`ref_slot == -1`) — return `(bytes base ptr, byte offset, is_array)`.
    /// The JIT hoists this once per never-reassigned receiver and emits a native 8B
    /// load of the tagged pointer + a `Value::Object`/`Value::Array` (or `Value::Null`
    /// for the `0` sentinel) register store, byte-identical to `read_inline_ref`,
    /// instead of calling `jit_field_get`. `None` (→ keep the helper) for a primitive,
    /// a side-table reference (`ref_slot ≥ 0`: closure/func/**string** — the string
    /// GcRef path stays on the helper), a struct-typed root (`TAG_UNKNOWN`), or an
    /// out-of-range slot. Reads only (no write barrier); the returned pointer is valid
    /// for the frame (non-moving GC + fixed `bytes` + caller holds the object live).
    #[inline]
    pub fn inline_ref_field(&self, name: &str) -> Option<(*const u8, u32, bool)> {
        let slot = *self.type_desc.field_index.get(name)?;
        let fa = self.field_access_of(slot)?;
        if fa.ref_slot >= 0 { return None; } // side-table reference (closure/func/string)
        match fa.tag {
            TAG_OBJECT => Some((self.bytes().as_ptr(), fa.offset, false)),
            TAG_ARRAY  => Some((self.bytes().as_ptr(), fa.offset, true)),
            _ => None, // primitive / struct root / string
        }
    }

    /// unify-object-byte-layout (PR-2): write direct field `slot` from `v`.
    /// Primitive → `encode_prim` into `bytes`; reference → the `refs` side-table cell.
    /// Returns `true` iff the target is a reference slot (so the caller fires a GC
    /// `write_barrier_field` when `v.is_heap_ref()`). No-op (returns `false`) for an
    /// out-of-range slot or struct-typed root. Replaces `self.slots[slot] = v`.
    #[inline]
    pub fn set_field_value(&mut self, slot: usize, v: &Value) -> bool {
        let fa = match self.field_access_of(slot) { Some(f) => f, None => return false };
        if fa.ref_slot >= 0 {
            if let Some(cell) = self.refs_mut().get_mut(fa.ref_slot as usize) { *cell = v.clone(); }
            return true;
        }
        // PR-3 chunk 2b: an inlined direct object/array reference — write the 8B tagged
        // pointer into `bytes` (`Null`/non-heap → 0). Returns `true` so the caller still
        // fires `write_barrier_field` (the target IS a reference slot, just byte-inlined).
        if fa.tag == TAG_OBJECT || fa.tag == TAG_ARRAY {
            write_inline_ref(&mut self.bytes_mut(), fa.offset as usize, v);
            return true;
        }
        if fa.tag == TAG_UNKNOWN { return false; } // struct-typed root
        // Reflection (FieldInfo/PropertyInfo SetValue) passes primitives **boxed**
        // (`int` → a `Std.Int32` `BoxedStruct`); a boxed primitive's bytes ARE its raw
        // scalar, so decode it with the field's tag/width to recover the plain `Value`
        // that `encode_prim` needs. Non-boxed values (the common FieldSet path from z42
        // code) pass through untouched.
        let unboxed: Value;
        let src: &Value = match v {
            Value::BoxedStruct(gc) => {
                let b = gc.borrow();
                if b.bytes().len() >= fa.width as usize {
                    unboxed = decode_prim(&b.bytes(), 0, fa.width as usize, fa.tag)
                        .unwrap_or(Value::Null);
                    &unboxed
                } else {
                    // Not a same-width primitive box — leave as-is (encode may reject).
                    v
                }
            }
            _ => v,
        };
        let _ = encode_prim(&mut self.bytes_mut(), fa.offset as usize, fa.width as usize, fa.tag, src);
        false
    }

    /// unify-object-byte-layout (PR-3 chunk 2b): visit the object's **byte-inlined**
    /// direct object/array references — the ones pulled out of `refs` into `bytes`.
    /// Reads each 8B tagged pointer at its `InlineRef::offset`, rebuilds the `Value`
    /// (`Object`/`Array` by `is_array`), and hands live (non-`Null`) ones to the GC
    /// visitor. Complements the `for r in &obj.refs` side-table scan; together they
    /// cover every reference edge. No-op for types without a composed object layout
    /// (value structs use `struct_layout`; synthesized layouts inline nothing).
    #[inline]
    pub fn trace_inline_refs(&self, visit: &mut dyn FnMut(&Value)) {
        if let Some(col) = self.type_desc.composed_object_layout() {
            for ir in col.inline_refs.iter() {
                let v = read_inline_ref(&self.bytes(), ir.offset as usize, ir.is_array);
                if !matches!(v, Value::Null) {
                    visit(&v);
                }
            }
        }
    }

    /// unify-object-byte-layout (PR-3 chunk 2b): zero every byte-inlined object/array
    /// reference window (→ the `0` `Null` sentinel). The `bytes` twin of nulling the
    /// `refs` side-table: used when finalizing/tombstoning an object to break strong
    /// reference edges (both the side-table `refs` and the inlined pointers must be
    /// cleared, else a cycle stays anchored through `bytes`).
    #[inline]
    pub fn clear_inline_refs(&mut self) {
        // Collect offsets first so the `type_desc` layout borrow is released before the
        // mutable `bytes` write (disjoint fields, but keeps the borrow checker happy).
        let offsets: Vec<u32> = match self.type_desc.composed_object_layout() {
            Some(col) if !col.inline_refs.is_empty() => {
                col.inline_refs.iter().map(|ir| ir.offset).collect()
            }
            _ => return,
        };
        for off in offsets {
            let off = off as usize;
            if off + 8 <= self.bytes().len() {
                self.bytes_mut()[off..off + 8].fill(0);
            }
        }
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

    /// **extract-typedesc-from-mutex (2026-05-31)**: lockless read of
    /// the object's `type_args` (generic type arguments at construction).
    /// Same write-once invariant as `type_desc` — set by `alloc_object`
    /// (per the spec, `alloc_object` accepts `type_args` and writes them
    /// before returning the GcRef), never mutated after.
    #[inline]
    pub fn type_args(&self) -> &[String] {
        // SAFETY: type_args is write-once-at-alloc; see type_desc().
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_args() }
    }
}

// ── Value ────────────────────────────────────────────────────────────────────
