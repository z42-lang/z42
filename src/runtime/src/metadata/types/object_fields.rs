//! 实例字段的单元读写与 GC 遍历（object model R1 / R2）。
//!
//! 一个直接字段落在下列单元之一（[`FieldCell`]，由加载期的 `ObjectLayout::field_access` 决定）：
//!
//! | 单元 | 位置 | 读 / 写 |
//! |---|---|---|
//! | 基元 | `bytes` 的组合偏移，原宽度 | relaxed 原子 |
//! | 引用字 | `bytes` 的组合偏移，8 B（`ref_word`） | acquire 读 / release 写；mark 期间 swap 取旧值给 SATB |
//! | 型参 | 标签字在组合偏移、负载字追加在编译器布局之后，各 8 B（`tparam_cell`） | 标签字 acquire / release，负载字 relaxed |
//! | 侧表 | `refs[aux]`，16 B `Value` | 只有合成布局（没有编译器对象块的类型）的引用字段 |
//!
//! 内联 struct 的引用叶子也在侧表（`objops::struct_leaf` 经 `set_ref_slot` 读写）。
//! `Value` 与单元之间的转换只在这里、`ref_word` 与 `tparam_cell` 里；两个引擎都经 `objops` 调到这些方法。

#![allow(unused_imports)]
use super::*;
use super::{ref_word, tparam_cell};
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
        match fa.cell() {
            // SAFETY: the word was written by `try_set_field_value` (or is the zero-initialised
            // `null`); its referent is kept alive by this object, which the caller holds.
            FieldCell::Ref => unsafe { ref_word::decode(self.storage.load_ref_word(fa.offset as usize)) },
            FieldCell::TypeParam => tparam_cell::load(&self.storage, fa.offset, fa.aux),
            FieldCell::Value => self.refs().get(fa.aux as usize).copied().unwrap_or(Value::Null),
            FieldCell::Prim => self.storage
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
        match fa.cell() {
            FieldCell::Ref => match ref_word::encode(v) {
                Some(w) => {
                    self.store_ref_cell(fa.offset, w);
                    Ok(FieldWrite::Ref)
                }
                None => {
                    let b = Self::box_for_ref_cell(v)?;
                    self.store_ref_cell(fa.offset, ref_word::encode_boxed(&b));
                    Ok(FieldWrite::Boxed(b))
                }
            },
            FieldCell::Prim => {
                match v {
                    // Reflection (`FieldInfo` / `PropertyInfo.SetValue`) passes primitives
                    // **boxed**; `FieldSet` passes them plain.
                    Value::BoxedStruct(_) => {
                        let src = Self::unbox_for_prim(v, fa);
                        self.storage.store_prim(fa.offset as usize, fa.width as usize, fa.tag, &src)?
                    }
                    _ => self.storage.store_prim(fa.offset as usize, fa.width as usize, fa.tag, v)?,
                }
                Ok(FieldWrite::NotRef)
            }
            FieldCell::TypeParam => tparam_cell::store(&mut self.storage, fa.offset, fa.aux, v),
            FieldCell::Value => {
                self.set_ref_slot(fa.aux as usize, v);
                Ok(FieldWrite::Ref)
            }
            FieldCell::Struct => Ok(FieldWrite::NotRef),
        }
    }

    /// The escape hatch of a reference word: a value that does not fit 8 B goes into a
    /// one-element box from the ambient heap.
    #[cold]
    #[inline(never)]
    fn box_for_ref_cell(v: &Value) -> anyhow::Result<GcRef<ArrayObj>> {
        ref_word::alloc_box_ambient(v)
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

    /// Store a reference word, handing the word it replaces to the SATB deletion barrier
    /// (a major mark that is running must still see the referent it cut).
    #[inline(always)]
    fn store_ref_cell(&mut self, off: u32, w: u64) {
        let old = self.storage.store_ref_word(off as usize, w);
        if old != 0 {
            // SAFETY: `old` was this cell's word an instant ago, so its referent is still live
            // (no safepoint in between).
            crate::gc::satb::record_overwrite(&unsafe { ref_word::decode_for_trace(old) });
        }
    }

    /// Store `v` into side-table leaf `ri`, recording the overwritten value for the SATB
    /// barrier. The mutator-side write for 16 B side-table leaves.
    #[inline]
    pub fn set_ref_slot(&mut self, ri: usize, v: &Value) {
        if let Some(cell) = self.storage.refs_mut_raw().get_mut(ri) {
            crate::gc::satb::record_overwrite(cell);
            *cell = *v;
        }
    }

    /// Visit every reference edge of this object: the 16 B side table, then each non-null
    /// 8 B reference word and each type-parameter cell's tag word holding a reference (decoded
    /// for tracing — a boxed value yields its box). The one traversal GC marking, root
    /// scanning of stack objects and the retention graph share.
    #[inline]
    pub fn visit_refs(&self, visit: &mut dyn FnMut(&Value)) {
        for r in self.refs() {
            visit(r);
        }
        if let Some(col) = self.type_desc.composed_object_layout_ref() {
            for &off in col.ref_cells.iter() {
                let w = self.storage.load_ref_word(off as usize);
                if w != 0 {
                    // SAFETY: a reference word of a reachable object names a live referent.
                    visit(&unsafe { ref_word::decode_for_trace(w) });
                }
            }
            for &off in col.tparam_cells.iter() {
                tparam_cell::visit(&self.storage, off, visit);
            }
        }
    }

    /// Break every strong reference edge of a dead object — side table, reference words and
    /// type-parameter cells — so no tombstoned entry keeps a handle into the region. GC-only:
    /// no barrier.
    pub fn clear_refs_for_sweep(&mut self) {
        for r in self.storage.refs_mut_raw().iter_mut() {
            *r = Value::Null;
        }
        // `type_desc` and `storage` are disjoint fields: the layout stays borrowed across the
        // writes without collecting the offsets first (this runs once per dead object).
        let Some(col) = self.type_desc.composed_object_layout_ref() else { return };
        for &off in col.ref_cells.iter() {
            self.storage.clear_ref_word(off as usize);
        }
        for &off in col.tparam_cells.iter() {
            tparam_cell::clear(&mut self.storage, off);
        }
    }
}
