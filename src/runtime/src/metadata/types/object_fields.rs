//! 实例字段的单元读写与 GC 遍历（object model R1 / R2）。
//!
//! 一个直接字段落在下列单元之一（[`FieldCell`]，由加载期的 `ObjectLayout::field_access` 决定）：
//!
//! | 单元 | 位置 | 读 / 写 |
//! |---|---|---|
//! | 基元 | `bytes` 的组合偏移，原宽度 | relaxed 原子 |
//! | 引用字 | `bytes` 的组合偏移，8 B（`ref_word`） | acquire 读 / release 写；mark 期间 swap 取旧值给 SATB |
//! | 型参 | 标签字在组合偏移、负载字追加在编译器布局之后，各 8 B（`tparam_cell`） | 标签字 acquire / release，负载字 relaxed |
//!
//! 合成布局（没有编译器对象块的类型）的引用字段一律是型参单元。内联 struct 与独立 blob 同布局：
//! 引用叶子也是引用字（`struct_blob`；`objops::struct_leaf` 经 `struct_ref_leaf` / `set_struct_ref_leaf`
//! 读写），装箱 struct 的 `bytes` 就是整块 blob。对象里没有 16 B `Value` 侧表。
//! `Value` 与单元之间的转换只在这里、`ref_word`、`tparam_cell` 与 `struct_blob` 里；两个引擎都经
//! `objops` 调到这些方法。

#![allow(unused_imports)]
use super::*;
use super::{ref_word, struct_blob, tparam_cell};
use crate::gc::GcRef;

/// What [`ScriptObject::try_set_field_value`] wrote — what the caller's write barrier needs.
#[derive(Debug, Clone, Copy)]
pub enum FieldWrite {
    /// A primitive cell, a primitive or `null` into a type-parameter cell, an inline struct
    /// root or a missing slot: no new heap edge, no barrier.
    NotRef,
    /// A reference cell now holds the value that was passed in.
    Ref,
    /// A reference word (or a type-parameter cell's tag word) now holds this one-element box
    /// of the value that was passed in (a raw primitive reaching an `object` field through
    /// erased generics; a value of another kind than a type-parameter cell already holds).
    Boxed(GcRef<ArrayObj>),
}

impl FieldWrite {
    #[inline]
    pub fn is_ref(&self) -> bool {
        !matches!(self, FieldWrite::NotRef)
    }

    /// Fire `barrier` with the heap reference that now sits in the cell, if any. `v` is the
    /// value that was written. By reference, so the hot path copies no `Value`.
    #[inline(always)]
    pub fn with_barrier_value(&self, v: &Value, barrier: impl FnOnce(&Value)) {
        match self {
            FieldWrite::Ref if v.is_heap_ref() => barrier(v),
            FieldWrite::Boxed(b) => barrier(&Value::Array(*b)),
            _ => {}
        }
    }
}

impl ScriptObject {
    /// The resolved `FieldAccess` for direct field `slot` (see `TypeDesc::field_index`).
    /// Reads the type's composed object layout; falls back to on-the-fly synthesis for a
    /// layout-less type (rare — synthetic / Rust-constructed).
    #[inline(always)]
    fn field_access_of(&self, slot: usize) -> Option<FieldAccess> {
        if let Some(col) = self.type_desc.composed_object_layout_ref() {
            return col.field_access.get(slot).copied();
        }
        self.synthesized_field_access(slot)
    }

    #[cold]
    #[inline(never)]
    fn synthesized_field_access(&self, slot: usize) -> Option<FieldAccess> {
        if self.type_desc.fields.is_empty() { return None; }
        synthesize_object_layout(&self.type_desc.fields).field_access.get(slot).copied()
    }

    /// Read direct field `slot` as a `Value`. `Null` for an out-of-range slot or an inline
    /// struct root (accessed via `StructFieldGetPrim`, never `FieldGet`).
    #[inline]
    pub fn field_value(&self, slot: usize) -> Value {
        let Some(fa) = self.field_access_of(slot) else { return Value::Null };
        let c = self.storage.cells();
        match fa.cell() {
            FieldCell::Ref => ref_word::load(&c, fa.offset as usize),
            FieldCell::TypeParam => tparam_cell::load(&c, fa.offset as usize, fa.aux as usize),
            FieldCell::Prim => c
                .load_prim(fa.offset as usize, fa.width as usize, fa.tag)
                .unwrap_or(Value::Null),
            FieldCell::Struct => Value::Null,
        }
    }

    /// JIT hoist: if `name` is a direct **primitive** field, `(bytes base ptr, byte offset,
    /// width, tag)` for a native width-aware load / store (a plain aligned access is the
    /// relaxed atomic on x86-64 / AArch64). `None` (→ keep the helper) for anything else.
    /// The pointer stays valid for the frame: non-moving GC, fixed payload block, receiver
    /// held live by the frame.
    #[inline]
    pub fn inline_prim_field(&self, name: &str) -> Option<(*const u8, u32, u32, u8)> {
        let slot = *self.type_desc.field_index.get(name)?;
        let fa = self.field_access_of(slot)?;
        (fa.cell() == FieldCell::Prim).then(|| (self.bytes().as_ptr(), fa.offset, fa.width, fa.tag))
    }

    /// JIT hoist: if `name` is a direct field held in an 8 B **reference word**, `(bytes base
    /// ptr, byte offset)`. The inline code acquire-loads the word and decodes the kind
    /// through `ref_word::KIND_TO_VALUE_TAG` (kind 7 → helper). Reads only.
    #[inline]
    pub fn inline_ref_field(&self, name: &str) -> Option<(*const u8, u32)> {
        let slot = *self.type_desc.field_index.get(name)?;
        let fa = self.field_access_of(slot)?;
        (fa.cell() == FieldCell::Ref).then(|| (self.bytes().as_ptr(), fa.offset))
    }

    /// Write direct field `slot` from `v`; `true` iff the target is a reference cell.
    ///
    /// **Infallible wrapper** over [`Self::try_set_field_value`]: a rejected primitive store
    /// is dropped. Use it ONLY where the value is made by the VM itself and cannot disagree
    /// with the slot (zero-init, test harnesses, stamping a stack trace). Any path that can
    /// receive a value chosen by user code — reflection, `FieldSet` — must call
    /// `try_set_field_value` and propagate.
    #[inline]
    pub fn set_field_value(&mut self, slot: usize, v: &Value) -> bool {
        matches!(self.try_set_field_value(slot, v), Ok(w) if w.is_ref())
    }

    /// Write direct field `slot` from `v`. The result tells the caller whether a reference
    /// cell was written and, when `v` could not be an 8 B reference word (a raw primitive
    /// reaching an `object` field through erased generics), the box that now sits there —
    /// the caller hands it to `write_barrier_field` via [`FieldWrite::with_barrier_value`].
    ///
    /// A rejected primitive store (`null` into an `int`, a wrong-typed value) is an error,
    /// never silently dropped: reflection's `SetValue` reaches this.
    #[inline]
    pub fn try_set_field_value(&mut self, slot: usize, v: &Value) -> anyhow::Result<FieldWrite> {
        let Some(fa) = self.field_access_of(slot) else { return Ok(FieldWrite::NotRef) };
        let c = self.storage.cells_mut();
        match fa.cell() {
            FieldCell::Ref => ref_word::store(&c, fa.offset as usize, v),
            FieldCell::Prim => {
                match v {
                    // Reflection (`FieldInfo` / `PropertyInfo.SetValue`) passes primitives
                    // **boxed**; `FieldSet` passes them plain.
                    Value::BoxedStruct(_) => {
                        let src = Self::unbox_for_prim(v, fa);
                        c.store_prim(fa.offset as usize, fa.width as usize, fa.tag, &src)?
                    }
                    _ => c.store_prim(fa.offset as usize, fa.width as usize, fa.tag, v)?,
                }
                Ok(FieldWrite::NotRef)
            }
            FieldCell::TypeParam => tparam_cell::store(&c, fa.offset as usize, fa.aux as usize, v),
            FieldCell::Struct => Ok(FieldWrite::NotRef),
        }
    }

    /// A boxed primitive's bytes ARE its raw scalar: decode them with the field's tag / width.
    /// Anything else (not a same-width primitive box) passes through and `store_prim` rejects it.
    #[cold]
    #[inline(never)]
    fn unbox_for_prim(v: &Value, fa: FieldAccess) -> Value {
        let Value::BoxedStruct(gc) = v else { return *v };
        let b = gc.borrow();
        if b.bytes().len() >= fa.width as usize {
            decode_prim(b.bytes(), 0, fa.width as usize, fa.tag).unwrap_or(Value::Null)
        } else {
            *v
        }
    }

    /// Visit every reference edge of this object: each non-null reference word and each
    /// type-parameter cell's tag word holding a reference, decoded for tracing (a boxed value
    /// yields its box). The one traversal GC marking, root scanning of stack objects and the
    /// retention graph share.
    #[inline]
    pub fn visit_refs(&self, visit: &mut dyn FnMut(&Value)) {
        let c = self.storage.cells();
        for_each_ref_cell(&self.type_desc, |cell| match cell {
            RefCell::Word(off) => ref_word::visit(&c, off, visit),
            RefCell::TypeParam(off) => tparam_cell::visit(&c, off, visit),
        });
    }

    /// Break every strong reference edge of a dead object so no tombstoned entry keeps a handle
    /// into the region. GC-only: no barrier.
    pub fn clear_refs_for_sweep(&mut self) {
        // `type_desc` and `storage` are disjoint fields: the layout stays borrowed across the
        // writes (this runs once per dead object).
        let c = self.storage.cells_mut();
        for_each_ref_cell(&self.type_desc, |cell| match cell {
            RefCell::Word(off) => ref_word::clear(&c, off),
            RefCell::TypeParam(off) => tparam_cell::clear(&c, off),
        });
    }
}

// ── Value structs inside an object: inline struct fields and the boxed struct's own blob ──────
impl ScriptObject {
    /// Whether the 8 B cell at `off` is a struct reference leaf of this object: an inline-struct
    /// leaf (or a direct reference field) of a class instance, or a reference leaf of a boxed
    /// struct's blob.
    fn is_struct_ref_leaf(&self, off: u32) -> bool {
        let td = &self.type_desc;
        if let Some(col) = td.composed_object_layout_ref() {
            return col.is_ref_cell(off);
        }
        td.is_struct() && td.struct_layout_ref().is_some_and(|sl| sl.ref_index(off).is_some())
    }

    #[cold]
    fn not_a_ref_leaf(&self, off: u32) -> anyhow::Error {
        anyhow::anyhow!("struct ref leaf at byte offset {off} not in the layout of `{}`", self.type_desc.name)
    }

    /// Read the struct reference leaf at byte `off` — a composed offset for an inline struct of a
    /// class instance, a blob offset for a boxed struct.
    pub fn struct_ref_leaf(&self, off: u32) -> anyhow::Result<Value> {
        if !self.is_struct_ref_leaf(off) {
            return Err(self.not_a_ref_leaf(off));
        }
        Ok(ref_word::load(&self.storage.cells(), off as usize))
    }

    /// Write the struct reference leaf at byte `off` (see [`Self::struct_ref_leaf`]). The result
    /// tells the caller which reference to hand to its write barrier.
    pub fn set_struct_ref_leaf(&mut self, off: u32, v: &Value) -> anyhow::Result<FieldWrite> {
        if !self.is_struct_ref_leaf(off) {
            return Err(self.not_a_ref_leaf(off));
        }
        ref_word::store(&self.storage.cells_mut(), off as usize, v)
    }

    /// Read the primitive leaf at byte `off` (width `w`, `ty::TAG_*` `kind`).
    #[inline]
    pub fn prim_leaf(&self, off: usize, w: usize, kind: u8) -> anyhow::Result<Value> {
        self.storage.cells().load_prim(off, w, kind)
    }

    /// Write the primitive leaf at byte `off`. Never a reference cell (that would plant raw bits
    /// where GC expects a reference word).
    #[inline]
    pub fn set_prim_leaf(&mut self, off: usize, w: usize, kind: u8, v: &Value) -> anyhow::Result<()> {
        debug_assert!(!self.is_struct_ref_leaf((off & !7) as u32),
            "primitive store over a reference cell at {off} of `{}`", self.type_desc.name);
        self.storage.cells_mut().store_prim(off, w, kind, v)
    }

    /// Snapshot the value struct of `size` bytes at `base` (an inline field; `0` for a boxed
    /// struct) into the frame-arena form — bytes with the reference slots zeroed, reference leaves
    /// at `base + ref_offsets` decoded in order (`struct_blob::read`).
    pub fn read_blob(&self, base: usize, size: usize, ref_offsets: &[u32]) -> (Vec<u8>, Vec<Value>) {
        struct_blob::read(&self.storage.cells(), base, size, ref_offsets)
    }

    /// Copy an arena-form value struct into the blob at `base` (`struct_blob::write`); each stored
    /// reference (a box, for a value that did not fit) goes to `barrier`.
    pub fn write_blob(
        &mut self, base: usize, size: usize, bytes: &[u8], refs: &[Value], ref_offsets: &[u32],
        barrier: &mut dyn FnMut(&Value),
    ) -> anyhow::Result<()> {
        struct_blob::write(&self.storage.cells_mut(), base, size, bytes, refs, ref_offsets, barrier)
    }

    /// A boxed struct's whole blob in the arena form (bytes with the reference slots zeroed + the
    /// reference leaves in `ref_offsets` order). A box of a type without a struct layout (a boxed
    /// primitive) yields its bytes and no references.
    pub fn boxed_blob(&self) -> (Vec<u8>, Vec<Value>) {
        match self.type_desc.struct_layout_ref() {
            Some(sl) => self.read_blob(0, self.storage.bytes().len(), &sl.ref_offsets),
            None => (self.storage.bytes().to_vec(), Vec::new()),
        }
    }
}

/// One reference-holding cell of an object (see [`ScriptObject::visit_refs`]).
#[derive(Clone, Copy)]
enum RefCell {
    /// An 8 B reference word at this byte offset.
    Word(usize),
    /// A type-parameter cell whose tag word is at this byte offset.
    TypeParam(usize),
}

/// The reference cells of an instance of `td`, by layout: the composed object layout (reference
/// words — direct fields and inline-struct leaves — and type-parameter cells), a boxed struct's
/// blob (the struct layout's reference leaves), or a synthesized layout (type-parameter cells).
#[inline]
fn for_each_ref_cell(td: &TypeDesc, mut f: impl FnMut(RefCell)) {
    if let Some(col) = td.composed_object_layout_ref() {
        for &off in col.ref_cells.iter() { f(RefCell::Word(off as usize)); }
        for &off in col.tparam_cells.iter() { f(RefCell::TypeParam(off as usize)); }
    } else if td.is_struct() {
        if let Some(sl) = td.struct_layout_ref() {
            for &off in sl.ref_offsets.iter() { f(RefCell::Word(off as usize)); }
        }
    } else if !td.fields.is_empty() {
        synthesized_ref_cells(td, &mut f);
    }
}

/// Layout-less class (Rust-constructed type, test fixture): its synthesized layout, built on the
/// fly — the loader gives every loaded class a composed layout, so this is cold.
#[cold]
#[inline(never)]
fn synthesized_ref_cells(td: &TypeDesc, f: &mut dyn FnMut(RefCell)) {
    for &off in synthesize_object_layout(&td.fields).tparam_cells.iter() {
        f(RefCell::TypeParam(off as usize));
    }
}

#[cfg(test)]
impl ScriptObject {
    /// Test-only: write field `slot` **without** the SATB record — the raw store GC unit tests use
    /// to wire object graphs (and to simulate a mutator that bypassed the barrier).
    pub(crate) fn set_ref_raw_for_test(&mut self, slot: usize, v: &Value) {
        use std::sync::atomic::Ordering;
        let Some(fa) = self.field_access_of(slot) else { return };
        let c = self.storage.cells_mut();
        match fa.cell() {
            FieldCell::Ref => {
                let w = ref_word::encode(v).expect("test value fits a reference word");
                c.store_word(fa.offset as usize, w, Ordering::Release);
            }
            FieldCell::TypeParam => {
                let (w0, w1) = tparam_cell::init_words(v).expect("test value fits a cell");
                c.store_word(fa.aux as usize, w1, Ordering::Relaxed);
                c.store_word(fa.offset as usize, w0, Ordering::Release);
            }
            _ => { let _ = self.try_set_field_value(slot, v); }
        }
    }
}
