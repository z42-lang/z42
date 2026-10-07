//! ArrayObj 视图 / GC 遍历 / 批量拷贝 / 深拷贝 / JIT 快路数据（构造在 `array.rs`，元素读写在
//! `array_elems.rs`）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

// ── 批量拷贝 ──
impl ArrayObj {
    /// Materialise all elements as a `Vec<Value>` (reflection, conversions).
    pub fn to_boxed_vec(&self) -> Vec<Value> {
        (0..self.len()).map(|i| self.get_boxed(i)).collect()
    }

    /// Iterate all elements as owned `Value`s.
    #[inline]
    pub fn iter_boxed(&self) -> impl Iterator<Item = Value> + '_ {
        (0..self.len()).map(move |i| self.get_boxed(i))
    }

    /// Bulk element move `src[si..si+n]` → `self[di..di+n]` (`src` is another array). Same
    /// packed backing → element by element at the element width (`Cells::copy_prims`); REF → REF → word by word (SATB on each overwritten
    /// word); two `struct[]` of the same element layout → blob by blob; any other pair →
    /// element-wise through `get_boxed` / `set_boxed`, so conversions stay exactly what the
    /// single-element path defines. A `struct[]` paired with a different backing needs boxing /
    /// unboxing and type checks — the heap-aware bulk path's job (debug-asserted here).
    /// Caller bounds-checks both ranges and fires the write barrier over the written range
    /// (`elem_ref_slots`).
    pub fn copy_elems_from(&self, src: &ArrayObj, si: usize, di: usize, n: usize) {
        // Same packed backing: element-wise relaxed atomics of exactly the element width (no
        // plain `memcpy` — another thread may be writing either array).
        if let (Some(sc), Some(dc), Some(w)) = (Self::prim_cells(&src.backing), Self::prim_cells(&self.backing), super::array_elems::prim_width_of(&self.backing)) {
            if std::mem::discriminant(&src.backing) == std::mem::discriminant(&self.backing) {
                dc.copy_prims(di, &sc, si, n, w);
                return;
            }
        }
        let done = match (&src.backing, &self.backing) {
            (ArrayBacking::Refs { block: sb, len: sl }, ArrayBacking::Refs { block: db, len: dl }) => {
                let s = Self::cells_of(sb, *sl * REF_STRIDE);
                let d = Self::cells_of(db, *dl * REF_STRIDE);
                for k in 0..n {
                    ref_word::store_word_recorded(&d, (di + k) * REF_STRIDE, s.load_ref_word((si + k) * REF_STRIDE));
                }
                true
            }
            (ArrayBacking::StructBytes { layout: sly, .. }, ArrayBacking::StructBytes { layout: dly, .. })
                if sly.size == dly.size && sly.ref_offsets == dly.ref_offsets =>
            {
                for k in 0..n {
                    if let Some((b, r)) = src.struct_elem(si + k) {
                        let _ = self.write_struct_elem(di + k, &b, &r, &mut |_| {});
                    }
                }
                true
            }
            (ArrayBacking::StructBytes { .. }, _) | (_, ArrayBacking::StructBytes { .. }) => {
                debug_assert!(false,
                    "copy_elems_from: struct[] paired with a different backing / layout needs the \
                     heap-aware bulk path (boxing / unboxing / type checks)");
                return;
            }
            _ => false,
        };
        if !done {
            for k in 0..n {
                self.set_boxed(di + k, src.get_boxed(si + k));
            }
        }
    }

    /// Same-array bulk move with `memmove` semantics: copies backward when the destination
    /// range starts above the source range. `di == si` is a no-op.
    pub fn copy_elems_within(&self, si: usize, di: usize, n: usize) {
        if di == si || n == 0 {
            return;
        }
        // Packed primitives: element-wise relaxed atomics in `memmove` direction.
        if let (Some(c), Some(w)) = (Self::prim_cells(&self.backing), super::array_elems::prim_width_of(&self.backing)) {
            c.copy_prims(di, &c, si, n, w);
            return;
        }
        let done = match &self.backing {
            ArrayBacking::Refs { block, len } => {
                let c = Self::cells_of(block, *len * REF_STRIDE);
                let mv = |k: usize| ref_word::store_word_recorded(
                    &c, (di + k) * REF_STRIDE, c.load_ref_word((si + k) * REF_STRIDE));
                if di > si { (0..n).rev().for_each(mv) } else { (0..n).for_each(mv) }
                true
            }
            ArrayBacking::StructBytes { .. } => {
                let mv = |k: usize| {
                    if let Some((b, r)) = self.struct_elem(si + k) {
                        let _ = self.write_struct_elem(di + k, &b, &r, &mut |_| {});
                    }
                };
                if di > si { (0..n).rev().for_each(mv) } else { (0..n).for_each(mv) }
                true
            }
            _ => false,
        };
        // MIXED / FROZEN / stack backings walk element-wise in the direction that does not
        // clobber elements yet to be read (`set_boxed` SATB-records each overwrite).
        if !done {
            let mv = |k: usize| { let v = self.get_boxed(si + k); self.set_boxed(di + k, v); };
            if di > si { (0..n).rev().for_each(mv) } else { (0..n).for_each(mv) }
        }
    }
}

// ── GC：引用遍历、backing 块的标记与年龄 ──
impl ArrayObj {
    /// Visit every reference edge this array holds (decoded for tracing — a boxed value yields
    /// its box): REF words, MIXED tag words holding a reference, FROZEN / stack `Value`s, and each
    /// `struct[]` element's reference leaves. The one traversal GC marking, stack-array root
    /// scanning and the retention graph share.
    pub fn visit_refs(&self, visit: &mut dyn FnMut(&Value)) {
        match &self.backing {
            ArrayBacking::Refs { block, len } => {
                let c = Self::cells_of(block, *len * REF_STRIDE);
                for i in 0..*len { ref_word::visit(&c, i * REF_STRIDE, visit); }
            }
            ArrayBacking::Cells { block, len, .. } => {
                let c = Self::cells_of(block, *len * CELL_STRIDE);
                for i in 0..*len { tparam_cell::visit(&c, i * CELL_STRIDE, visit); }
            }
            // SAFETY: a live FROZEN block of `len` Values.
            ArrayBacking::Frozen { block, len } =>
                for v in unsafe { Self::slice_of::<Value>(block, *len) } { visit(v); },
            ArrayBacking::StackVec(v) => for e in v { visit(e); },
            ArrayBacking::StructBytes { block, len, layout } => {
                if layout.ref_offsets.is_empty() { return; }
                let c = Self::cells_of(block, *len * layout.size);
                for i in 0..*len { struct_blob::visit(&c, i * layout.size, &layout.ref_offsets, visit); }
            }
            _ => {}
        }
    }

    /// The reference slots elements `[i, i+n)` own — each slot's current content decoded for
    /// tracing (a box shows as the box) — plus how many slots each element has: REF / MIXED /
    /// FROZEN / stack → 1 each; `struct[]` → the element's reference leaves (`ref_count`, possibly
    /// 0); packed primitives → none. What a bulk store's write barrier walks. Caller bounds-checks.
    pub fn elem_ref_slots(&self, i: usize, n: usize) -> (Vec<Value>, usize) {
        use std::sync::atomic::Ordering;
        let trace = |w: u64| if w & ref_word::KIND_MASK != 0 {
            // SAFETY: a reference word of a live array names a live referent.
            unsafe { ref_word::decode_for_trace(w) }
        } else { Value::Null };
        match &self.backing {
            ArrayBacking::Refs { block, len } => {
                let c = Self::cells_of(block, *len * REF_STRIDE);
                ((i..i + n).map(|k| trace(c.load_ref_word(k * REF_STRIDE))).collect(), 1)
            }
            ArrayBacking::Cells { block, len, .. } => {
                let c = Self::cells_of(block, *len * CELL_STRIDE);
                ((i..i + n).map(|k| trace(c.load_word(k * CELL_STRIDE, Ordering::Acquire))).collect(), 1)
            }
            ArrayBacking::Frozen { .. } | ArrayBacking::StackVec(_) =>
                ((i..i + n).map(|k| self.get_boxed(k)).collect(), 1),
            ArrayBacking::StructBytes { block, len, layout } => {
                let c = Self::cells_of(block, *len * layout.size);
                let mut out = Vec::with_capacity(n * layout.ref_offsets.len());
                for k in i..i + n {
                    for &ro in layout.ref_offsets.iter() {
                        out.push(trace(c.load_ref_word(k * layout.size + ro as usize)));
                    }
                }
                (out, layout.ref_offsets.len())
            }
            _ => (Vec::new(), 1),
        }
    }

    /// Visit the references held in elements `[i, i+n)` — `(element index, reference decoded for
    /// tracing)`, skipping `null` and primitives — without materialising the slots (what a bulk
    /// store's write barrier walks; a `List<int>` growth copy visits nothing and allocates nothing).
    pub fn visit_elem_refs(&self, i: usize, n: usize, mut f: impl FnMut(usize, &Value)) {
        use std::sync::atomic::Ordering;
        let mut word = |k: usize, w: u64| if w & ref_word::KIND_MASK != 0 && ref_word::handle_bits(w) != 0 {
            // SAFETY: a reference word of a live array names a live referent.
            f(k, &unsafe { ref_word::decode_for_trace(w) });
        };
        match &self.backing {
            ArrayBacking::Refs { block, len } => {
                let c = Self::cells_of(block, *len * REF_STRIDE);
                for k in i..i + n { word(k, c.load_ref_word(k * REF_STRIDE)); }
            }
            ArrayBacking::Cells { block, len, .. } => {
                let c = Self::cells_of(block, *len * CELL_STRIDE);
                for k in i..i + n { word(k, c.load_word(k * CELL_STRIDE, Ordering::Acquire)); }
            }
            ArrayBacking::StructBytes { block, len, layout } => {
                let c = Self::cells_of(block, *len * layout.size);
                for k in i..i + n {
                    for &ro in layout.ref_offsets.iter() { word(k, c.load_ref_word(k * layout.size + ro as usize)); }
                }
            }
            ArrayBacking::Frozen { .. } | ArrayBacking::StackVec(_) => {
                for k in i..i + n {
                    let v = self.get_boxed(k);
                    if v.is_heap_ref() { f(k, &v); }
                }
            }
            _ => {}
        }
    }

    /// The single GC block behind this array (`None` for a stack array).
    #[inline]
    fn backing_block(&self) -> Option<&VarGcRef> {
        match &self.backing {
            ArrayBacking::Refs { block, .. }
            | ArrayBacking::Cells { block, .. }
            | ArrayBacking::Frozen { block, .. }
            | ArrayBacking::Bool { block, .. }
            | ArrayBacking::Bytes { block, .. }
            | ArrayBacking::I32 { block, .. }
            | ArrayBacking::I64 { block, .. }
            | ArrayBacking::Chars { block, .. }
            | ArrayBacking::F64 { block, .. }
            | ArrayBacking::StructBytes { block, .. } => Some(block),
            ArrayBacking::StackVec(_) => None,
        }
    }

    /// Test-only: the backing block's `gen_age` (`u8::MAX` for a stack array). Pins the "backing
    /// ages with its owner" invariant.
    pub fn min_backing_gen_age_for_test(&self) -> u8 {
        self.backing_block().map_or(u8::MAX, |b| b.gen_age())
    }

    /// Raise the backing block to at least `age` — called when the **array header** is promoted,
    /// so the element storage is never younger than the header that exclusively owns it (the
    /// block is kept alive only by [`Self::mark_backing`], not as a traced child, so a minor that
    /// skips a clean old array would otherwise lose a young block).
    pub fn raise_backing_gen_age(&self, age: u8) {
        if let Some(b) = self.backing_block() { b.raise_gen_age_to(age); }
    }

    /// Mark the backing block live during the GC mark phase (called right after the header is
    /// marked — without it the element block in `region_var` would be swept from under a live
    /// array).
    #[inline]
    pub fn mark_backing(&self, kind: crate::gc::refs::MarkKind) {
        if let Some(b) = self.backing_block() { b.mark(kind); }
    }
}

// ── 深拷贝 / JIT 快路数据 / 统计 ──
impl ArrayObj {
    /// An independent heap copy (`__array_clone`): a fresh block with the same bytes —
    /// reference words copied verbatim share their referents, exactly a shallow element copy.
    /// The copy is unreachable until its header exists, so no barrier is owed.
    pub fn deep_copy(&self, heap: &dyn MagrGC) -> Self {
        let copy = |b: &VarGcRef, bytes: usize, bt: BlockType| Self::alloc_block_copy(heap, b, bytes, bt);
        let backing = match &self.backing {
            ArrayBacking::Refs { block, len } => ArrayBacking::Refs { block: copy(block, len * REF_STRIDE, BlockType::ArrayRef), len: *len },
            ArrayBacking::Cells { block, len, zero } =>
                ArrayBacking::Cells { block: copy(block, len * CELL_STRIDE, BlockType::ArrayCell), len: *len, zero: *zero },
            ArrayBacking::Frozen { block, len } =>
                ArrayBacking::Frozen { block: copy(block, len * std::mem::size_of::<Value>(), BlockType::ArrayValue), len: *len },
            ArrayBacking::Bool { block, len } => ArrayBacking::Bool { block: copy(block, *len, BlockType::ArrayPrim), len: *len },
            ArrayBacking::Bytes { block, len } => ArrayBacking::Bytes { block: copy(block, *len, BlockType::ArrayPrim), len: *len },
            ArrayBacking::I32 { block, len } => ArrayBacking::I32 { block: copy(block, len * 4, BlockType::ArrayPrim), len: *len },
            ArrayBacking::Chars { block, len } => ArrayBacking::Chars { block: copy(block, len * 4, BlockType::ArrayPrim), len: *len },
            ArrayBacking::I64 { block, len } => ArrayBacking::I64 { block: copy(block, len * 8, BlockType::ArrayPrim), len: *len },
            ArrayBacking::F64 { block, len } => ArrayBacking::F64 { block: copy(block, len * 8, BlockType::ArrayPrim), len: *len },
            ArrayBacking::StructBytes { block, len, layout } => ArrayBacking::StructBytes {
                block: copy(block, len * layout.size, BlockType::ArrayStruct), len: *len, layout: layout.clone(),
            },
            // A stack array escaping through a copy becomes a MIXED heap array.
            ArrayBacking::StackVec(v) => Self::pack_backing_iter(heap, ElemKind::Mixed, v.len(), v.iter().copied()),
        };
        Self { element_type: self.element_type, backing: backing }
    }

    /// Snapshot `n` elements of a packed `byte[]` from element `off` on — an atomic chunked copy
    /// ([`Cells::copy_out`]), never a plain slice over memory another thread may be writing.
    /// `None` when this is not a `byte[]` or the range is out of bounds (FFI / I/O send paths).
    pub fn byte_range(&self, off: usize, n: usize) -> Option<Vec<u8>> {
        let ArrayBacking::Bytes { block, len } = &self.backing else { return None };
        let mut out = vec![0u8; n];
        Self::cells_of(block, *len).copy_out(off, &mut out).then_some(out)
    }

    /// The whole of a packed `byte[]` ([`Self::byte_range`]); `None` for any other backing.
    #[inline]
    pub fn byte_snapshot(&self) -> Option<Vec<u8>> {
        self.byte_range(0, self.len())
    }

    /// Copy `src` into a packed `byte[]` from element `off` on (relaxed atomic chunked stores,
    /// [`Cells::copy_in`]). `false` (nothing written) when this is not a `byte[]` or the range is
    /// out of bounds (I/O receive paths).
    pub fn write_byte_range(&self, off: usize, src: &[u8]) -> bool {
        let ArrayBacking::Bytes { block, len } = &self.backing else { return false };
        Self::cells_of(block, *len).copy_in(off, src)
    }

    /// JIT packed-numeric fast path: the element buffer of an `I32` / `I64` / `F64` / `Chars`
    /// backing (paired with [`Self::packed_elem_width`]). `None` for every other backing — the
    /// JIT set path sees width 0 and falls back to the helper. The block is fixed-size and
    /// non-moving, so the JIT may cache the pointer across the function.
    #[inline]
    pub fn packed_num_ptr(&self) -> Option<*const u8> {
        match &self.backing {
            ArrayBacking::I32 { block, .. } | ArrayBacking::I64 { block, .. }
            | ArrayBacking::F64 { block, .. } | ArrayBacking::Chars { block, .. } => {
                debug_assert!(block.is_live(), "packed_num_ptr on a stale/tombstoned block");
                // SAFETY: payload ptr from the raw header (D8); the JIT reads `len` slots only.
                Some(unsafe { block.payload_as_ptr::<u8>() } as *const u8)
            }
            _ => None,
        }
    }

    /// Packed slot width in bytes for the JIT fast path: 4 (`I32` / `Chars`), 8 (`I64` / `F64`),
    /// or 0 for any other backing (route to the helper).
    #[inline]
    pub fn packed_elem_width(&self) -> i64 {
        match &self.backing {
            ArrayBacking::I32 { .. } | ArrayBacking::Chars { .. } => 4,
            ArrayBacking::I64 { .. } | ArrayBacking::F64 { .. } => 8,
            _ => 0,
        }
    }

    /// JIT reference-read fast path: the element buffer, stride and zero kind of a REF (8 — one
    /// reference word per element, zero kind 0) or MIXED (16 — the tag word first; `zero` = the
    /// primitive kind a never-written cell reads as) backing. The inline code acquire-loads the
    /// (tag) word at `ptr + i * stride` and decodes it: a reference word through
    /// `ref_word::KIND_TO_VALUE_TAG`, a MIXED primitive marker through
    /// `tparam_cell::PRIM_KIND_TO_VALUE_TAG` plus the payload word; a box (kind 7) falls back to the
    /// helper. `None` for every other backing.
    #[inline]
    pub fn ref_elem_data(&self) -> Option<(*const u8, i64, u8)> {
        let (block, stride, zero) = match &self.backing {
            ArrayBacking::Refs { block, .. } => (block, REF_STRIDE, 0),
            ArrayBacking::Cells { block, zero, .. } => (block, CELL_STRIDE, *zero),
            _ => return None,
        };
        debug_assert!(block.is_live(), "ref_elem_data on a stale/tombstoned block");
        // SAFETY: payload ptr from the raw header (D8); the JIT reads `len` elements only.
        Some((unsafe { block.payload_as_ptr::<u8>() } as *const u8, stride as i64, zero))
    }

    /// Heap bytes for element storage (`len × element size`).
    #[inline]
    pub fn elem_storage_bytes(&self) -> usize {
        use std::mem::size_of;
        match &self.backing {
            ArrayBacking::Refs { len, .. } => len * REF_STRIDE,
            ArrayBacking::Cells { len, .. } => len * CELL_STRIDE,
            ArrayBacking::Frozen { len, .. } => len * size_of::<Value>(),
            ArrayBacking::Bool { len, .. } | ArrayBacking::Bytes { len, .. } => *len,
            ArrayBacking::I32 { len, .. } | ArrayBacking::Chars { len, .. } => len * 4,
            ArrayBacking::I64 { len, .. } | ArrayBacking::F64 { len, .. } => len * 8,
            ArrayBacking::StructBytes { len, layout, .. } => len * layout.size,
            ArrayBacking::StackVec(v) => v.len() * size_of::<Value>(),
        }
    }
}

#[cfg(test)]
impl ArrayObj {
    /// Test-only: a MIXED array whose element block is a **leaked** standalone GC block (never
    /// in a region, never swept) — for heap-less unit tests.
    pub(crate) fn new_leaked(elems: Vec<Value>) -> Self {
        let len = elems.len();
        let block = VarGcRef::leak_block_for_test(len * CELL_STRIDE, BlockType::ArrayCell);
        let a = Self { element_type: ElemType::empty(), backing: ArrayBacking::Cells { block, len, zero: 0 } };
        for (i, v) in elems.into_iter().enumerate() { a.set_boxed(i, v); }
        a
    }

    /// Test-only: a zeroed `StructBytes` array on a **leaked** block (elements written via
    /// `write_struct_elem`). For heap-less struct[] unit tests.
    pub(crate) fn struct_backed_leaked(element_type: &str, len: usize, layout: std::sync::Arc<StructTypeLayout>) -> Self {
        let block = VarGcRef::leak_block_for_test(len * layout.size, BlockType::ArrayStruct);
        Self { element_type: ElemType::intern(element_type), backing: ArrayBacking::StructBytes { len, block, layout } }
    }
}
