//! ArrayObj 元素访问 / 视图 / GC 标记 / 深拷贝。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

// ── 元素访问（ArrayObj 的 impl 续：构造 / backing 分配在 array.rs）──
impl ArrayObj {
    #[inline]
    pub fn len(&self) -> usize {
        match &self.backing {
            ArrayBacking::Boxed { len, .. }
            | ArrayBacking::Bool { len, .. }
            | ArrayBacking::Bytes { len, .. }
            | ArrayBacking::I32 { len, .. }
            | ArrayBacking::I64 { len, .. }
            | ArrayBacking::Chars { len, .. }
            | ArrayBacking::F64 { len, .. }
            | ArrayBacking::StructBytes { len, .. } => *len,
            ArrayBacking::StackVec(v) => v.len(),
        }
    }
    #[inline]
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    /// Bounds-checked read as owned `Value` (packed-safe `Vec::get` analogue).
    #[inline]
    pub fn get(&self, i: usize) -> Option<Value> {
        if i < self.len() { Some(self.get_boxed(i)) } else { None }
    }
    #[inline]
    pub fn first(&self) -> Option<Value> { self.get(0) }

    /// Read element `i` as a `Value` (boxes packed primitives). Caller ensures
    /// `i < len()`. SAFETY of block reads: see [`Self::slice_of`] (held under `&self`
    /// = shared region lock).
    #[inline]
    pub fn get_boxed(&self, i: usize) -> Value {
        match &self.backing {
            // SAFETY (each arm): shared borrow of a live block of exactly `len` `T`s.
            ArrayBacking::Boxed { block, len } => (unsafe { Self::slice_of::<Value>(block, *len) })[i].clone(),
            ArrayBacking::Bool { block, len }  => Value::Bool((unsafe { Self::slice_of::<bool>(block, *len) })[i]),
            ArrayBacking::Bytes { block, len } => Value::I64((unsafe { Self::slice_of::<u8>(block, *len) })[i] as i64),
            ArrayBacking::I32 { block, len }   => Value::I64((unsafe { Self::slice_of::<i32>(block, *len) })[i] as i64),
            ArrayBacking::I64 { block, len }   => Value::I64((unsafe { Self::slice_of::<i64>(block, *len) })[i]),
            ArrayBacking::Chars { block, len } => Value::Char((unsafe { Self::slice_of::<char>(block, *len) })[i]),
            ArrayBacking::F64 { block, len }   => Value::F64((unsafe { Self::slice_of::<f64>(block, *len) })[i]),
            // Escape-analysis stack array: boxed Values inline in the arena Vec.
            ArrayBacking::StackVec(v) => v[i].clone(),
            // add-struct-heap-inline (P3b): reading a struct[] element as a generic
            // `Value` yields a **boxed copy** (value semantics — the read is a snapshot;
            // mutating the box does not touch the array). In-place `arr[i].x = v` /
            // `arr[i].x` leaf access instead goes through a `Value::StructRefHeap`
            // handle at the exec layer (it has the array `GcRef`; `get_boxed` does not).
            // add-boxed-struct-identity (P4b): boxing a struct[] element now requires a
            // heap allocation (the box is a shared `ScriptObject`), which this
            // `&self` accessor cannot do. The value path never reaches here — interp
            // `array_get` + jit array-get materialize a `StructRefHeap` handle for
            // `StructBytes` backing (see exec_array.rs / jit/helpers/array.rs), and any
            // real struct→object boxing goes through `__box_struct` (heap-aware). This
            // arm is an invariant guard; if a materialization path ever needs a boxed
            // struct[] element, route it through a `ctx`-carrying helper, not `get_boxed`.
            ArrayBacking::StructBytes { .. } => {
                debug_assert!(false,
                    "get_boxed on a StructBytes backing: struct[] element boxing needs a heap-aware path, not get_boxed");
                Value::Null
            }
        }
    }
    /// Write `Value` into element `i` (unboxes into packed primitives). Caller
    /// ensures `i < len()`. SAFETY of block writes: [`Self::slice_of_mut`] (held under
    /// `&mut self` = exclusive region lock).
    #[inline]
    pub fn set_boxed(&mut self, i: usize, val: Value) {
        match &mut self.backing {
            // SAFETY (each arm): exclusive borrow of a live block of exactly `len` `T`s.
            ArrayBacking::Boxed { block, len } => { let s = unsafe { Self::slice_of_mut::<Value>(block, *len) }; s[i] = val; }
            ArrayBacking::Bool { block, len }  => { let s = unsafe { Self::slice_of_mut::<bool>(block, *len) }; s[i] = matches!(val, Value::Bool(true)); }
            ArrayBacking::Bytes { block, len } => { let s = unsafe { Self::slice_of_mut::<u8>(block, *len) }; s[i] = if let Value::I64(n) = val { n as u8 } else { 0 }; }
            ArrayBacking::I32 { block, len }   => { let s = unsafe { Self::slice_of_mut::<i32>(block, *len) }; s[i] = if let Value::I64(n) = val { n as i32 } else { 0 }; }
            ArrayBacking::I64 { block, len }   => { let s = unsafe { Self::slice_of_mut::<i64>(block, *len) }; s[i] = if let Value::I64(n) = val { n } else { 0 }; }
            ArrayBacking::Chars { block, len } => { let s = unsafe { Self::slice_of_mut::<char>(block, *len) }; s[i] = if let Value::Char(c) = val { c } else { '\0' }; }
            ArrayBacking::F64 { block, len }   => { let s = unsafe { Self::slice_of_mut::<f64>(block, *len) }; s[i] = if let Value::F64(f) = val { f } else { 0.0 }; }
            // Escape-analysis stack array: store the boxed Value directly in the arena Vec.
            ArrayBacking::StackVec(v) => v[i] = val,
            // add-struct-heap-inline (P3b): writing a whole struct[] element from a
            // **boxed** source copies its bytes + reference leaves into the element slot.
            // A frame-scoped `StructRef` source needs `ctx.struct_arena` → handled at the
            // exec-layer `ArraySet` (this generic setter only sees `&mut self`).
            ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } => {
                if let Value::BoxedStruct(b) = &val {
                    // add-boxed-struct-identity (P4b): read the source box's blob out of
                    // its shared `ScriptObject` (borrow needs no `ctx`).
                    let bo = b.borrow();
                    let rc = layout.ref_count();
                    let bstart = i * *elem_size;
                    let n = bo.bytes.len().min(*elem_size);
                    // SAFETY: exclusive block payloads: `bytes` holds `len*elem_size` u8,
                    // `refs` holds `len*rc` Values.
                    let bslice = unsafe { Self::slice_of_mut::<u8>(bytes, *len * *elem_size) };
                    bslice[bstart..bstart + n].copy_from_slice(&bo.bytes[..n]);
                    let rslice = unsafe { Self::slice_of_mut::<Value>(refs, *len * rc) };
                    let rn = bo.refs.len().min(rc);
                    for k in 0..rn { rslice[i * rc + k] = bo.refs[k].clone(); }
                } else {
                    debug_assert!(false,
                        "struct[] set_boxed needs a BoxedStruct source (StructRef → exec-level ArraySet), got {val:?}");
                }
            }
        }
    }

    /// unify-gc-heap PR-3: copy one `struct[]` element's packed bytes + reference leaves
    /// into slot `i` of a `StructBytes` backing. Used by the exec-layer struct-array literal
    /// packer (`pack_struct_elem`), which resolves `BoxedStruct` / `StructRef` sources into
    /// `(bytes, refs)` first (it can't reach the private block accessors). No-op on other
    /// backings. Caller holds `&mut self` = exclusive block access.
    pub fn write_struct_elem(&mut self, i: usize, src_bytes: &[u8], src_refs: &[Value]) {
        if let ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } = &mut self.backing {
            let rc = layout.ref_count();
            let bstart = i * *elem_size;
            let n = src_bytes.len().min(*elem_size);
            // SAFETY: exclusive block payloads: `bytes` = `len*elem_size` u8, `refs` = `len*rc` Values.
            let bslice = unsafe { Self::slice_of_mut::<u8>(bytes, *len * *elem_size) };
            bslice[bstart..bstart + n].copy_from_slice(&src_bytes[..n]);
            let rslice = unsafe { Self::slice_of_mut::<Value>(refs, *len * rc) };
            let rn = src_refs.len().min(rc);
            for k in 0..rn { rslice[i * rc + k] = src_refs[k].clone(); }
        }
    }

    /// unify-gc-heap PR-3: the `StructBytes` element type layout (element `elem_size`
    /// = `layout.size`, `ref_count`, `ref_index`). `None` for non-struct[] backings.
    /// Returns a cloned `Arc` so the caller can drop the shared borrow before taking a
    /// `&mut` block slice (`struct_bytes_mut` / `struct_refs_mut`).
    #[inline]
    pub fn struct_layout(&self) -> Option<std::sync::Arc<StructTypeLayout>> {
        match &self.backing {
            ArrayBacking::StructBytes { layout, .. } => Some(layout.clone()),
            _ => None,
        }
    }
    /// unify-gc-heap PR-3: the whole packed-bytes region of a `StructBytes` array
    /// (`len*elem_size` bytes) for struct[] leaf prim decode. `None` otherwise.
    #[inline]
    pub fn struct_bytes(&self) -> Option<&[u8]> {
        match &self.backing {
            // SAFETY: shared borrow of a live ArrayStruct block of `len*elem_size` bytes.
            ArrayBacking::StructBytes { bytes, len, elem_size, .. } =>
                Some(unsafe { Self::slice_of::<u8>(bytes, *len * *elem_size) }),
            _ => None,
        }
    }
    /// Mutable packed-bytes region of a `StructBytes` array (struct[] leaf prim encode).
    #[inline]
    pub fn struct_bytes_mut(&mut self) -> Option<&mut [u8]> {
        match &mut self.backing {
            // SAFETY: exclusive borrow of a live ArrayStruct block of `len*elem_size` bytes.
            ArrayBacking::StructBytes { bytes, len, elem_size, .. } =>
                Some(unsafe { Self::slice_of_mut::<u8>(bytes, *len * *elem_size) }),
            _ => None,
        }
    }
    /// Mutable reference side-table of a `StructBytes` array (`len*ref_count` Values) —
    /// struct[] reference-leaf writes. `None` otherwise. (Reads use `gc_refs()`.)
    #[inline]
    pub fn struct_refs_mut(&mut self) -> Option<&mut [Value]> {
        match &mut self.backing {
            // SAFETY: exclusive borrow of a live ArrayValue block of `len*ref_count` Values.
            ArrayBacking::StructBytes { refs, len, layout, .. } =>
                Some(unsafe { Self::slice_of_mut::<Value>(refs, *len * layout.ref_count()) }),
            _ => None,
        }
    }
}

// ── 视图 / GC / 深拷贝（第二个 impl 块：code-organization 200 行类型限制）──
impl ArrayObj {
    /// Materialise all elements as a `Vec<Value>` (for sites needing a boxed
    /// snapshot — reflection, conversions). Boxes packed primitives.
    pub fn to_boxed_vec(&self) -> Vec<Value> {
        (0..self.len()).map(|i| self.get_boxed(i)).collect()
    }

    /// add-struct-heap-inline (P3b): every heap reference this array holds, for the
    /// GC mark traversal. A `Boxed` array's elements are all refs; a `StructBytes`
    /// (value-struct) array's refs are the inline elements' reference leaves in the
    /// side-table (packed primitives in `bytes` hold none). Packed-primitive arrays
    /// return `&[]`. The returned slice borrows the backing block for `&self`'s
    /// lifetime (block outlives the header).
    #[inline]
    pub fn gc_refs(&self) -> &[Value] {
        match &self.backing {
            // SAFETY: shared borrow of a live ArrayValue block of `len` Values.
            ArrayBacking::Boxed { block, len } => unsafe { Self::slice_of::<Value>(block, *len) },
            // SAFETY: shared borrow of a live ArrayValue block of `len*ref_count` Values.
            ArrayBacking::StructBytes { refs, len, layout, .. } =>
                unsafe { Self::slice_of::<Value>(refs, *len * layout.ref_count()) },
            // Stack array: elements are boxed Values in the arena Vec, scanned as roots.
            ArrayBacking::StackVec(v) => v,
            _ => &[],
        }
    }

    /// unify-gc-heap PR-3: mark this array's backing block(s) live during the GC
    /// mark phase. Called from `Value::trace_children`'s array-borrowing arms right
    /// after the `ArrayObj` header (region_array) is marked — without this the
    /// element blocks in `region_var` would be swept out from under a live array.
    #[inline]
    pub fn mark_backing(&self) {
        match &self.backing {
            ArrayBacking::Boxed { block, .. }
            | ArrayBacking::Bool { block, .. }
            | ArrayBacking::Bytes { block, .. }
            | ArrayBacking::I32 { block, .. }
            | ArrayBacking::I64 { block, .. }
            | ArrayBacking::Chars { block, .. }
            | ArrayBacking::F64 { block, .. } => { block.mark(); }
            ArrayBacking::StructBytes { bytes, refs, .. } => { bytes.mark(); refs.mark(); }
            // Stack array: no GC block — the arena Vec is scanned as a root, nothing to mark.
            ArrayBacking::StackVec(_) => {}
        }
    }

    /// unify-gc-heap PR-3: an independent heap-allocated copy (value-semantic array
    /// clone — `__array_clone`). Allocates fresh backing block(s) in `heap` and copies
    /// element data in (cloning `Value`s), so the copy shares nothing mutable with the
    /// original. Replaces the removed `#[derive(Clone)]` (which would have aliased the
    /// backing block).
    pub fn deep_copy(&self, heap: &dyn MagrGC) -> Self {
        let backing = match &self.backing {
            ArrayBacking::Boxed { block, len } => {
                let src = unsafe { Self::slice_of::<Value>(block, *len) };
                ArrayBacking::Boxed { block: Self::alloc_values_clone(heap, src), len: *len }
            }
            ArrayBacking::Bool { block, len } =>
                ArrayBacking::Bool { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<bool>(block, *len) }), len: *len },
            ArrayBacking::Bytes { block, len } =>
                ArrayBacking::Bytes { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<u8>(block, *len) }), len: *len },
            ArrayBacking::I32 { block, len } =>
                ArrayBacking::I32 { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<i32>(block, *len) }), len: *len },
            ArrayBacking::I64 { block, len } =>
                ArrayBacking::I64 { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<i64>(block, *len) }), len: *len },
            ArrayBacking::Chars { block, len } =>
                ArrayBacking::Chars { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<char>(block, *len) }), len: *len },
            ArrayBacking::F64 { block, len } =>
                ArrayBacking::F64 { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<f64>(block, *len) }), len: *len },
            ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } => {
                let rc = layout.ref_count();
                let bsrc = unsafe { Self::slice_of::<u8>(bytes, *len * *elem_size) };
                let rsrc = unsafe { Self::slice_of::<Value>(refs, *len * rc) };
                ArrayBacking::StructBytes {
                    elem_size: *elem_size,
                    len: *len,
                    bytes: Self::alloc_packed(heap, bsrc),
                    refs: Self::alloc_values_clone(heap, rsrc),
                    layout: layout.clone(),
                }
            }
            // A stack array being deep-copied escapes into the heap → materialize its boxed
            // elements into a fresh GC `Boxed` block (never hit in practice: `__array_clone`
            // only sees heap `Value::Array`, but keep the copy heap-correct if it ever does).
            ArrayBacking::StackVec(v) => {
                let (block, len) = Self::alloc_boxed(heap, v.clone());
                ArrayBacking::Boxed { block, len }
            }
        };
        Self { element_type: self.element_type.clone(), backing }
    }

    /// Zero-copy packed byte slice for FFI (`Some` iff `byte[]`). Step 3 uses
    /// this to hand native code a contiguous `&[u8]` — no per-byte marshal.
    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match &self.backing {
            // SAFETY: shared borrow of a live ArrayPrim block of `len` bytes.
            ArrayBacking::Bytes { block, len } => Some(unsafe { Self::slice_of::<u8>(block, *len) }),
            _ => None,
        }
    }

    /// JIT packed-numeric fast path: `I32`/`I64`/`F64` backings are contiguous
    /// fixed-width slots (4 / 8 / 8 bytes) the JIT can index with a native
    /// stride-N load/store — no 24-byte `Value` round-trip, no per-element tag.
    /// Pairs with [`Self::packed_elem_width`]: the ptr is the buffer base, the
    /// width tells the JIT the slot size (4 → `int[]` sign-extends into the i64
    /// payload; 8 → raw `long[]`/`double[]` copy). `None` for `Boxed`/`Bytes`/
    /// `Bool`/`Chars` — the JIT set-path detects width 0 and falls back to the
    /// `jit_array_set` helper, so those backings never index off this ptr.
    ///
    /// unify-gc-heap PR-3: the ptr is now the GC block's inline payload (non-moving,
    /// fixed-size) instead of a `Vec` buffer — the JIT may cache it across the
    /// function (blocks don't relocate; A' is a non-moving allocator).
    #[inline]
    pub fn packed_num_ptr(&self) -> Option<*const u8> {
        match &self.backing {
            // SAFETY: block payload ptr from the raw header (D8); JIT reads `len` slots only.
            ArrayBacking::I32 { block, .. }
            | ArrayBacking::I64 { block, .. }
            | ArrayBacking::F64 { block, .. }
            // jit-inline-char-arrays: `char` is a 4-byte scalar (Rust `char` == u32);
            // the JIT loads it width-4 and boxes into `Value::Char`.
            | ArrayBacking::Chars { block, .. } => {
                debug_assert!(block.is_live(), "unify-gc-heap PR-3: packed_num_ptr on a stale/tombstoned block");
                Some(unsafe { block.payload_as_ptr::<u8>() } as *const u8)
            }
            _ => None,
        }
    }

    /// Packed slot width in bytes for the JIT fast path: 4 (`I32`/`Chars`), 8
    /// (`I64`/`F64`), or 0 for a non-packed backing (`Boxed`/`Bytes`/`Bool`).
    /// The **runtime** authority the JIT ArraySet inline consults so a narrowing
    /// store (`int[i] = <i64 value>`) writes the right slot size rather than
    /// trusting the value register's width. Width 0 → route to the helper.
    #[inline]
    pub fn packed_elem_width(&self) -> i64 {
        match &self.backing {
            ArrayBacking::I32 { .. } | ArrayBacking::Chars { .. } => 4,
            ArrayBacking::I64 { .. } | ArrayBacking::F64 { .. } => 8,
            _ => 0,
        }
    }

    /// Iterate all elements as owned `Value`s (boxes packed primitives).
    /// Packed-safe replacement for the old `Deref`→`Vec<Value>` `.iter()`.
    #[inline]
    pub fn iter_boxed(&self) -> impl Iterator<Item = Value> + '_ {
        (0..self.len()).map(move |i| self.get_boxed(i))
    }

    /// Heap bytes for element storage (`len × sizeof(element)`) — the packed-array
    /// memory win shows up here (byte[] 1B vs Boxed 24B/elem). unify-gc-heap PR-3:
    /// counts the GC block payload(s); arrays are fixed-size so `len == capacity`.
    #[inline]
    pub fn elem_storage_bytes(&self) -> usize {
        use std::mem::size_of;
        match &self.backing {
            ArrayBacking::Boxed { len, .. } => len * size_of::<Value>(),
            ArrayBacking::Bool { len, .. }  => *len,
            ArrayBacking::Bytes { len, .. } => *len,
            ArrayBacking::I32 { len, .. }   => len * 4,
            ArrayBacking::I64 { len, .. }   => len * 8,
            ArrayBacking::Chars { len, .. } => len * 4,
            ArrayBacking::F64 { len, .. }   => len * 8,
            // Packed struct bytes + the reference side-table (16B/handle in a Value).
            ArrayBacking::StructBytes { elem_size, len, layout, .. } =>
                len * elem_size + len * layout.ref_count() * size_of::<Value>(),
            ArrayBacking::StackVec(v) => v.len() * size_of::<Value>(),
        }
    }
}

#[cfg(test)]
impl ArrayObj {
    /// Test-only: a `Boxed` array whose element block is a **leaked** standalone GC block
    /// (never in a region, never swept) — for heap-less unit tests that need a heap-backed
    /// array without wiring an `ArcMagrGC`. Mirrors `VarGcRef::leak_for_test` (used by these
    /// same tests for closures). Never run under Miri's leak checker.
    pub(crate) fn new_leaked(elems: Vec<Value>) -> Self {
        let len = elems.len();
        let block = VarGcRef::leak_block_for_test(len * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh leaked block sized for `len` Values; move each in over the POD zero.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in elems.into_iter().enumerate() { unsafe { base.add(i).write(v); } }
        Self { element_type: Arc::from(""), backing: ArrayBacking::Boxed { block, len } }
    }

    /// Test-only: a zero-/`Null`-initialized `StructBytes` array with **leaked** byte + ref
    /// blocks (elements written via `write_struct_elem`). For heap-less struct[] unit tests.
    pub(crate) fn struct_backed_leaked(element_type: &str, len: usize, layout: std::sync::Arc<StructTypeLayout>) -> Self {
        let elem_size = layout.size;
        let rc = layout.ref_count();
        let bytes = VarGcRef::leak_block_for_test(len * elem_size, BlockType::ArrayStruct);
        let refs = VarGcRef::leak_block_for_test(len * rc * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh leaked ref block sized for `len*rc` Values; Null-init each slot.
        let rbase = unsafe { refs.payload_as_ptr::<Value>() };
        for i in 0..len * rc { unsafe { rbase.add(i).write(Value::Null); } }
        Self { element_type: Arc::from(element_type), backing: ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } }
    }
}
