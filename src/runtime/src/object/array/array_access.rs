//! ArrayObj 视图 / GC 遍历 / 批量拷贝 / 深拷贝 / JIT 快路数据（构造在 `array.rs`，元素读写在
//! `array_elems.rs`）。

#![allow(unused_imports)]
use crate::object::*;
use std::sync::Arc;
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
    /// packed mode → element by element at the element width (`Cells::copy_prims`); REF → REF →
    /// word by word (SATB on each overwritten word); two `struct[]` of the same element layout →
    /// blob by blob; any other pair → element-wise through `get_boxed` / `set_boxed`, so
    /// conversions stay exactly what the single-element path defines. A `struct[]` paired with a
    /// different mode needs boxing / unboxing and type checks — the heap-aware bulk path's job
    /// (debug-asserted here). Caller bounds-checks both ranges and fires the write barrier over
    /// the written range.
    pub fn copy_elems_from(&self, src: &ArrayObj, si: usize, di: usize, n: usize) {
        let (sm, dm) = (src.mode(), self.mode());
        // Same packed mode: element-wise relaxed atomics of exactly the element width (no plain
        // `memcpy` — another thread may be writing either array).
        if sm == dm && dm.is_prim() {
            self.cells().copy_prims(di, &src.cells(), si, n, self.width());
            return;
        }
        match (sm, dm) {
            (ArrayMode::Ref, ArrayMode::Ref) => {
                let (s, d) = (src.cells(), self.cells());
                for k in 0..n {
                    ref_word::store_word_recorded(&d, (di + k) * REF_STRIDE, s.load_ref_word((si + k) * REF_STRIDE));
                }
            }
            (ArrayMode::Struct, ArrayMode::Struct) => {
                let same = match (src.struct_layout_ref(), self.struct_layout_ref()) {
                    (Some(a), Some(b)) => a.size == b.size && a.ref_offsets == b.ref_offsets,
                    _ => false,
                };
                debug_assert!(same, "copy_elems_from: two struct[] with different element layouts");
                if same {
                    for k in 0..n {
                        if let Some((b, r)) = src.struct_elem(si + k) {
                            let _ = self.write_struct_elem(di + k, &b, &r, &mut |_| {});
                        }
                    }
                }
            }
            (ArrayMode::Struct, _) | (_, ArrayMode::Struct) => debug_assert!(false,
                "copy_elems_from: struct[] paired with a different mode needs the heap-aware bulk \
                 path (boxing / unboxing / type checks)"),
            _ => for k in 0..n {
                self.set_boxed(di + k, src.get_boxed(si + k));
            },
        }
    }

    /// Same-array bulk move with `memmove` semantics: copies backward when the destination
    /// range starts above the source range. `di == si` is a no-op.
    pub fn copy_elems_within(&self, si: usize, di: usize, n: usize) {
        if di == si || n == 0 {
            return;
        }
        let m = self.mode();
        // Packed primitives: element-wise relaxed atomics in `memmove` direction.
        if m.is_prim() {
            let c = self.cells();
            c.copy_prims(di, &c, si, n, self.width());
            return;
        }
        let dir = |mv: &dyn Fn(usize)| if di > si { (0..n).rev().for_each(mv) } else { (0..n).for_each(mv) };
        match m {
            ArrayMode::Ref => {
                let c = self.cells();
                dir(&|k| ref_word::store_word_recorded(&c, (di + k) * REF_STRIDE, c.load_ref_word((si + k) * REF_STRIDE)));
            }
            ArrayMode::Struct => dir(&|k| {
                if let Some((b, r)) = self.struct_elem(si + k) {
                    let _ = self.write_struct_elem(di + k, &b, &r, &mut |_| {});
                }
            }),
            // MIXED / FROZEN walk element-wise in the direction that does not clobber elements
            // yet to be read (`set_boxed` SATB-records each overwrite).
            _ => dir(&|k| { let v = self.get_boxed(si + k); self.set_boxed(di + k, v); }),
        }
    }
}

// ── GC：引用遍历 ──
impl ArrayObj {
    /// Visit every reference edge this array holds (decoded for tracing — a boxed value yields
    /// its box): REF words, MIXED tag words holding a reference, FROZEN `Value`s, and each
    /// `struct[]` element's reference leaves. The one traversal GC marking, the remembered set
    /// and the retention graph share.
    pub fn visit_refs(&self, visit: &mut dyn FnMut(&Value)) {
        match self.mode() {
            ArrayMode::Ref => {
                let c = self.cells();
                for i in 0..self.len() { ref_word::visit(&c, i * REF_STRIDE, visit); }
            }
            ArrayMode::Mixed => {
                let c = self.cells();
                for i in 0..self.len() { tparam_cell::visit(&c, i * CELL_STRIDE, visit); }
            }
            ArrayMode::Frozen => for v in self.frozen_values() { visit(v); },
            ArrayMode::Struct => {
                let Some((c, l)) = self.struct_cells() else { return };
                if l.ref_offsets.is_empty() { return; }
                for i in 0..self.len() { struct_blob::visit(&c, i * l.size, &l.ref_offsets, visit); }
            }
            _ => {}
        }
    }

    /// The reference slots elements `[i, i+n)` own — each slot's current content decoded for
    /// tracing (a box shows as the box) — plus how many slots each element has: REF / MIXED /
    /// FROZEN → 1 each; `struct[]` → the element's reference leaves (`ref_count`, possibly 0);
    /// packed primitives → none. Caller bounds-checks.
    pub fn elem_ref_slots(&self, i: usize, n: usize) -> (Vec<Value>, usize) {
        use std::sync::atomic::Ordering;
        let trace = |w: u64| if w & ref_word::KIND_MASK != 0 {
            // SAFETY: a reference word of a live array names a live referent.
            unsafe { ref_word::decode_for_trace(w) }
        } else { Value::Null };
        let c = self.cells();
        match self.mode() {
            ArrayMode::Ref => ((i..i + n).map(|k| trace(c.load_ref_word(k * REF_STRIDE))).collect(), 1),
            ArrayMode::Mixed => ((i..i + n).map(|k| trace(c.load_word(k * CELL_STRIDE, Ordering::Acquire))).collect(), 1),
            ArrayMode::Frozen => ((i..i + n).map(|k| self.get_boxed(k)).collect(), 1),
            ArrayMode::Struct => {
                let Some(l) = self.struct_layout_ref() else { return (Vec::new(), 0) };
                let mut out = Vec::with_capacity(n * l.ref_offsets.len());
                for k in i..i + n {
                    for &ro in l.ref_offsets.iter() {
                        out.push(trace(c.load_ref_word(k * l.size + ro as usize)));
                    }
                }
                (out, l.ref_offsets.len())
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
        let c = self.cells();
        match self.mode() {
            ArrayMode::Ref => for k in i..i + n { word(k, c.load_ref_word(k * REF_STRIDE)); },
            ArrayMode::Mixed => for k in i..i + n { word(k, c.load_word(k * CELL_STRIDE, Ordering::Acquire)); },
            ArrayMode::Struct => {
                let Some(l) = self.struct_layout_ref() else { return };
                for k in i..i + n {
                    for &ro in l.ref_offsets.iter() { word(k, c.load_ref_word(k * l.size + ro as usize)); }
                }
            }
            ArrayMode::Frozen => for k in i..i + n {
                let v = self.get_boxed(k);
                if v.is_heap_ref() { f(k, &v); }
            },
            _ => {}
        }
    }
}

// ── 深拷贝 / 字节区间 / JIT 快路数据 ──
impl ArrayObj {
    /// An independent heap copy (`__array_clone`): a fresh block with the same descriptor and
    /// bytes — reference words copied verbatim share their referents, exactly a shallow element
    /// copy. The source is read with atomic chunked loads (another thread may be writing it); the
    /// copy is unreachable until it is published, so no barrier is owed.
    pub fn deep_copy(&self, heap: &dyn MagrGC) -> Value {
        let src = self.cells();
        heap.alloc_array_with(self.desc(), self.len(), self.struct_layout(), &mut |a| {
            let n = src.len();
            if n == 0 { return; }
            // SAFETY: the copy's element area is fresh (nobody can see it yet) and exactly as
            // long as the source's — same descriptor, same length, same layout.
            let dst = unsafe { std::slice::from_raw_parts_mut(a.elems_ptr(), n) };
            src.copy_out(0, dst);
        })
    }

    /// Snapshot `n` elements of a packed `byte[]` from element `off` on — an atomic chunked copy
    /// ([`Cells::copy_out`]), never a plain slice over memory another thread may be writing.
    /// `None` when this is not a `byte[]` or the range is out of bounds (FFI / I/O send paths).
    pub fn byte_range(&self, off: usize, n: usize) -> Option<Vec<u8>> {
        if self.mode() != ArrayMode::Bytes { return None; }
        let mut out = vec![0u8; n];
        self.cells().copy_out(off, &mut out).then_some(out)
    }

    /// The whole of a packed `byte[]` ([`Self::byte_range`]); `None` for any other mode.
    #[inline]
    pub fn byte_snapshot(&self) -> Option<Vec<u8>> {
        self.byte_range(0, self.len())
    }

    /// Copy `src` into a packed `byte[]` from element `off` on (relaxed atomic chunked stores,
    /// [`Cells::copy_in`]). `false` (nothing written) when this is not a `byte[]` or the range is
    /// out of bounds (I/O receive paths).
    pub fn write_byte_range(&self, off: usize, src: &[u8]) -> bool {
        self.mode() == ArrayMode::Bytes && self.cells().copy_in(off, src)
    }

    /// JIT packed-numeric fast path: `(element base, length, slot width)` of an `int` / `long` /
    /// `double` / `char` array (width 4 / 8), or `(null, 0, 0)` for every other mode — the inline
    /// code then routes to the helper. Shape from the header's kind word (no descriptor load), one branch: the JIT asks this per
    /// access when the array is not loop-invariant. The block is fixed-size and non-moving, so the
    /// JIT may cache the pointer across the function.
    #[inline(always)]
    pub fn jit_packed(&self) -> (*const u8, i64, i64) {
        let m = self.mode();
        match m {
            ArrayMode::I32 | ArrayMode::Chars | ArrayMode::I64 | ArrayMode::F64 => {
                // Not STRUCT: the elements start at the payload (no layout word).
                let p = std::cell::UnsafeCell::raw_get(self.payload.as_ptr()) as *const u8;
                (p, (self.payload.len() >> m.shift()) as i64, 1 << m.shift())
            }
            _ => (std::ptr::null(), 0, 0),
        }
    }

    /// JIT reference-read fast path: `(element base, length, stride | zero << 8)` of a REF (stride
    /// 8 — one reference word per element, zero kind 0) or MIXED (16 — the tag word first; `zero` =
    /// the primitive kind a never-written cell reads as) array; `(null, 0, 0)` for every other
    /// mode. The inline code acquire-loads the (tag) word at `ptr + i * stride` and decodes it: a
    /// reference word through `ref_word::KIND_TO_VALUE_TAG`, a MIXED primitive marker through
    /// `tparam_cell::PRIM_KIND_TO_VALUE_TAG` plus the payload word; a box (kind 7) falls back to
    /// the helper. Like [`Self::jit_packed`], from the header alone.
    #[inline(always)]
    pub fn jit_refs(&self) -> (*const u8, i64, i64) {
        let m = self.mode();
        match m {
            ArrayMode::Ref | ArrayMode::Mixed => {
                let p = std::cell::UnsafeCell::raw_get(self.payload.as_ptr()) as *const u8;
                (p, (self.payload.len() >> m.shift()) as i64, (1i64 << m.shift()) | (self.zero() as i64) << 8)
            }
            _ => (std::ptr::null(), 0, 0),
        }
    }
}
