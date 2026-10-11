//! ArrayObj 元素读写：每种模式（REF / MIXED / PRIM / STRUCT / FROZEN / 栈数组）一份，读写都是同宽原子
//! 单元访问（object model R3，见 `array.rs` 模块文档）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::gc::GcRef;
use crate::gc::var_region::VarGcRef;

/// Packed element width of a PRIM backing (`None` for the others).
#[inline(always)]
pub(super) fn prim_width_of(b: &ArrayBacking) -> Option<usize> {
    Some(match b {
        ArrayBacking::Bool { .. } | ArrayBacking::Bytes { .. } => 1,
        ArrayBacking::I32 { .. } | ArrayBacking::Chars { .. } => 4,
        ArrayBacking::I64 { .. } | ArrayBacking::F64 { .. } => 8,
        _ => return None,
    })
}

impl ArrayObj {
    #[inline]
    pub fn len(&self) -> usize {
        match &self.backing {
            ArrayBacking::Refs { len, .. }
            | ArrayBacking::Cells { len, .. }
            | ArrayBacking::Frozen { len, .. }
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
    /// Bounds-checked read as owned `Value`.
    #[inline]
    pub fn get(&self, i: usize) -> Option<Value> {
        if i < self.len() { Some(self.get_boxed(i)) } else { None }
    }
    #[inline]
    pub fn first(&self) -> Option<Value> { self.get(0) }

    /// Read element `i` as a `Value` (boxes packed primitives, decodes reference words and
    /// type-parameter cells). Caller ensures `i < len()`.
    #[inline]
    pub fn get_boxed(&self, i: usize) -> Value {
        match &self.backing {
            ArrayBacking::Refs { block, len } =>
                ref_word::load(&Self::cells_of(block, len * REF_STRIDE), i * REF_STRIDE),
            ArrayBacking::Cells { block, len, zero } => tparam_cell::load_or_zero(
                &Self::cells_of(block, len * CELL_STRIDE), i * CELL_STRIDE, i * CELL_STRIDE + 8, *zero),
            // SAFETY: a live FROZEN block of `len` Values, written before publication.
            ArrayBacking::Frozen { block, len } => (unsafe { Self::slice_of::<Value>(block, *len) })[i],
            ArrayBacking::StackVec(v) => v[i],
            ArrayBacking::StructBytes { .. } => {
                // A struct[] element read as a generic `Value` needs a heap box (value semantics);
                // `get_boxed` cannot allocate. The value paths materialize a `StructRefHeap`
                // handle (`objops::array::array_get`) or box through the heap-aware bulk path.
                debug_assert!(false,
                    "get_boxed on a StructBytes backing: struct[] element boxing needs a heap-aware path");
                Value::Null
            }
            b => {
                let w = prim_width_of(b).unwrap_or(8);
                let bits = Self::prim_cells(b).and_then(|c| c.load_bits(i * w, w)).unwrap_or(0);
                match b {
                    ArrayBacking::Bool { .. } => Value::Bool(bits != 0),
                    ArrayBacking::Bytes { .. } => Value::I64(bits as u8 as i64),
                    ArrayBacking::I32 { .. } => Value::I64(bits as u32 as i32 as i64),
                    ArrayBacking::I64 { .. } => Value::I64(bits as i64),
                    ArrayBacking::Chars { .. } => Value::Char(char::from_u32(bits as u32).unwrap_or('\0')),
                    _ => Value::F64(f64::from_bits(bits)),
                }
            }
        }
    }

    /// Atomic loads and stores over a PRIM backing's block.
    #[inline(always)]
    pub(super) fn prim_cells(b: &ArrayBacking) -> Option<Cells<'_>> {
        let w = prim_width_of(b)?;
        match b {
            ArrayBacking::Bool { block, len } | ArrayBacking::Bytes { block, len }
            | ArrayBacking::I32 { block, len } | ArrayBacking::I64 { block, len }
            | ArrayBacking::Chars { block, len } | ArrayBacking::F64 { block, len } =>
                Some(Self::cells_of(block, len * w)),
            _ => None,
        }
    }

    /// 基元 backing 的**种类标签**（`None` = 非基元 backing）。标签文本同时用于错误消息。
    #[inline]
    pub fn prim_backing_kind(&self) -> Option<&'static str> {
        match &self.backing {
            ArrayBacking::Bool  { .. } => Some("bool[]"),
            ArrayBacking::Bytes { .. } => Some("byte[]"),
            ArrayBacking::I32   { .. } => Some("int[]"),
            ArrayBacking::I64   { .. } => Some("long[]"),
            ArrayBacking::Chars { .. } => Some("char[]"),
            ArrayBacking::F64   { .. } => Some("double[]"),
            _ => None,
        }
    }

    /// 「这个 `Value` 能**原样**存进本 backing 吗」——**严格同种**，不做拓宽（`double[]` 不收 `I64`）。
    /// 🔴 这是该问题的唯一判据，`store_elem` 的基元臂必须与它一致
    /// （`prim_backing_accepts_agrees_with_set_boxed` 逐格钉住）。
    /// ⚠️ `int[]` 收 `I64` 是正常路径：IR 整数一律 i64，存入时按 `as i32` 截断。
    #[inline]
    pub fn prim_backing_accepts(&self, val: &Value) -> bool {
        match &self.backing {
            ArrayBacking::Bool  { .. } => matches!(val, Value::Bool(_)),
            ArrayBacking::Bytes { .. } | ArrayBacking::I32 { .. } | ArrayBacking::I64 { .. } => matches!(val, Value::I64(_)),
            ArrayBacking::Chars { .. } => matches!(val, Value::Char(_)),
            ArrayBacking::F64   { .. } => matches!(val, Value::F64(_)),
            _ => true,
        }
    }

    /// 与 [`Self::set_boxed`] 同语义，但**类型不符时报错**而不是静默存 0 —— 无类型 API
    /// （`Std.Array.SetValue(Object, int)`、跨元素类型的 `CopyRange`）用它：值是用户给的，没有编译器
    /// 站点能先转换（`fix-silent-array-elem-zero`）。
    #[inline]
    pub fn try_set_boxed(&self, i: usize, val: Value) -> anyhow::Result<FieldWrite> {
        if !self.prim_backing_accepts(&val) {
            let kind = self.prim_backing_kind().unwrap_or("<non-primitive>");
            anyhow::bail!(
                "cannot store {} into a {kind} element: the value's kind does not match the \
                 array's element type (a silent zero would be indistinguishable from a \
                 legitimate write)",
                crate::semantics::value_kind_name(&val)
            );
        }
        self.store_elem(i, &val)
    }

    /// Write `val` into element `i` (unboxes into packed primitives, encodes reference words /
    /// type-parameter cells). Caller ensures `i < len()`. The result names the reference that
    /// landed (a box, for a value that did not fit) — the caller hands it to
    /// `write_barrier_array_elem` via [`FieldWrite::with_barrier_value`]. A whole `struct[]`
    /// element store reports `NotRef`; its caller barriers the element's leaves
    /// (`elem_ref_slots`).
    ///
    /// A primitive kind mismatch from an IR site stores a zero after a debug assert
    /// ([`prim_value_mismatch`]); a box that cannot be allocated (no heap) leaves the element
    /// untouched after a debug assert. Untyped APIs use [`Self::try_set_boxed`].
    #[inline]
    pub fn set_boxed(&self, i: usize, val: Value) -> FieldWrite {
        match self.store_elem(i, &val) {
            Ok(w) => w,
            Err(e) => {
                debug_assert!(false, "array store of {val:?}: {e}");
                FieldWrite::NotRef
            }
        }
    }

    /// [`Self::set_boxed`] through an exclusive borrow — the one store a **stack array**
    /// (`StackVec`, owned by the per-context stack arena and reached only under its lock) takes;
    /// every other backing forwards to [`Self::set_boxed`].
    #[inline]
    pub fn set_boxed_exclusive(&mut self, i: usize, val: Value) -> FieldWrite {
        if let ArrayBacking::StackVec(v) = &mut self.backing {
            v[i] = val;
            return FieldWrite::NotRef;
        }
        self.set_boxed(i, val)
    }

    /// The element store behind [`Self::set_boxed`] / [`Self::try_set_boxed`]. `&self`: a heap
    /// array is shared between threads without a lock, every store is one atomic cell write
    /// (object model R4). A FROZEN array refuses (written once, before publication); a stack
    /// array needs the arena's exclusive borrow ([`Self::set_boxed_exclusive`]).
    pub fn store_elem(&self, i: usize, val: &Value) -> anyhow::Result<FieldWrite> {
        match &self.backing {
            ArrayBacking::Refs { block, len } =>
                ref_word::store(&Self::cells_of(block, *len * REF_STRIDE), i * REF_STRIDE, val),
            ArrayBacking::Cells { block, len, zero } => tparam_cell::store_or_zero(
                &Self::cells_of(block, *len * CELL_STRIDE), i * CELL_STRIDE, i * CELL_STRIDE + 8, val, *zero),
            // A 16 B `Value` cannot be stored atomically: FROZEN blocks are written once, before
            // the array is published (closure environments, `ref_word` kind-7 boxes), and are
            // read-only afterwards.
            ArrayBacking::Frozen { .. } => anyhow::bail!("cannot store into a frozen array (closure environment / value box)"),
            ArrayBacking::StackVec(_) => anyhow::bail!("stack array store needs the stack arena's exclusive borrow"),
            ArrayBacking::StructBytes { .. } => self.store_struct_elem_boxed(i, val),
            b => {
                let w = prim_width_of(b).unwrap_or(8);
                let bits = match (b, val) {
                    (ArrayBacking::Bool { .. }, Value::Bool(x)) => *x as u64,
                    (ArrayBacking::Bytes { .. } | ArrayBacking::I32 { .. } | ArrayBacking::I64 { .. }, Value::I64(n)) => *n as u64,
                    (ArrayBacking::Chars { .. }, Value::Char(c)) => *c as u64,
                    (ArrayBacking::F64 { .. }, Value::F64(f)) => f.to_bits(),
                    _ => {
                        prim_value_mismatch(val, self.prim_backing_kind().unwrap_or("?"), "set_boxed");
                        0
                    }
                };
                if let Some(c) = Self::prim_cells(b) {
                    c.store_bits(i * w, w, bits);
                }
                Ok(FieldWrite::NotRef)
            }
        }
    }
}

// ── STRUCT：struct[] 元素（整块 blob 与单个叶子）──
impl ArrayObj {
    /// The `StructBytes` element layout; `None` for any other backing. A cloned `Arc`, so the
    /// caller can drop the borrow first.
    #[inline]
    pub fn struct_layout(&self) -> Option<std::sync::Arc<StructTypeLayout>> {
        match &self.backing {
            ArrayBacking::StructBytes { layout, .. } => Some(layout.clone()),
            _ => None,
        }
    }

    /// Atomic loads over a `struct[]` block (`None` for other backings).
    #[inline]
    pub(super) fn struct_cells(&self) -> Option<(Cells<'_>, &StructTypeLayout)> {
        match &self.backing {
            ArrayBacking::StructBytes { block, len, layout } =>
                Some((Self::cells_of(block, *len * layout.size), layout)),
            _ => None,
        }
    }

    /// Element `i` of a `struct[]` in the frame-arena form: its bytes (reference slots zeroed)
    /// and its reference leaves in `ref_offsets` order. `None` for any other backing. Caller
    /// bounds-checks `i`.
    pub fn struct_elem(&self, i: usize) -> Option<(Vec<u8>, Vec<Value>)> {
        let (c, l) = self.struct_cells()?;
        Some(struct_blob::read(&c, i * l.size, l.size, &l.ref_offsets))
    }

    /// Copy an arena-form struct `(bytes, refs)` into element `i` of a `struct[]` (no-op on any
    /// other backing). Every reference that landed goes to `barrier`.
    pub fn write_struct_elem(
        &self, i: usize, src_bytes: &[u8], src_refs: &[Value], barrier: &mut dyn FnMut(&Value),
    ) -> anyhow::Result<()> {
        if let ArrayBacking::StructBytes { block, len, layout } = &self.backing {
            let c = Self::cells_of(block, *len * layout.size);
            struct_blob::write(&c, i * layout.size, layout.size, src_bytes, src_refs, &layout.ref_offsets, barrier)?;
        }
        Ok(())
    }

    /// Whole-element store from a `BoxedStruct` source (the box's blob is the element's layout).
    /// A frame-scoped `StructRef` source needs the struct arena and is handled at the exec layer.
    fn store_struct_elem_boxed(&self, i: usize, val: &Value) -> anyhow::Result<FieldWrite> {
        let Value::BoxedStruct(b) = val else {
            anyhow::bail!("struct[] element store needs a boxed struct, got {val:?}");
        };
        let (bytes, refs) = b.borrow().boxed_blob();
        self.write_struct_elem(i, &bytes, &refs, &mut |_| {})?;
        Ok(FieldWrite::NotRef)
    }

    /// Read the reference leaf at blob offset `off` of element `i`.
    pub fn struct_elem_ref(&self, i: usize, off: u32) -> anyhow::Result<Value> {
        let (c, l) = self.struct_cells()
            .ok_or_else(|| anyhow::anyhow!("struct[] leaf read on a non-value-struct array"))?;
        if l.ref_index(off).is_none() {
            anyhow::bail!("struct[] ref leaf at byte offset {off} not in element layout");
        }
        Ok(ref_word::load(&c, i * l.size + off as usize))
    }

    /// Write the reference leaf at blob offset `off` of element `i` (SATB on the old word).
    pub fn set_struct_elem_ref(&self, i: usize, off: u32, v: &Value) -> anyhow::Result<FieldWrite> {
        let ArrayBacking::StructBytes { block, len, layout } = &self.backing else {
            anyhow::bail!("struct[] leaf write on a non-value-struct array");
        };
        if layout.ref_index(off).is_none() || i >= *len {
            anyhow::bail!("struct[] ref leaf at byte offset {off} of element {i} not in element layout");
        }
        ref_word::store(&Self::cells_of(block, *len * layout.size), i * layout.size + off as usize, v)
    }

    /// Read the primitive leaf at blob offset `off` (width `w`, tag `kind`) of element `i`.
    pub fn struct_elem_prim(&self, i: usize, off: u32, w: usize, kind: u8) -> anyhow::Result<Value> {
        let (c, l) = self.struct_cells()
            .ok_or_else(|| anyhow::anyhow!("struct[] leaf read on a non-value-struct array"))?;
        c.load_prim(i * l.size + off as usize, w, kind)
    }

    /// Write the primitive leaf at blob offset `off` of element `i`.
    pub fn set_struct_elem_prim(&self, i: usize, off: u32, w: usize, kind: u8, v: &Value) -> anyhow::Result<()> {
        let ArrayBacking::StructBytes { block, len, layout } = &self.backing else {
            anyhow::bail!("struct[] leaf write on a non-value-struct array");
        };
        debug_assert!(layout.ref_index(off & !7).is_none(), "primitive store over a struct[] reference leaf");
        Self::cells_of(block, *len * layout.size).store_prim(i * layout.size + off as usize, w, kind, v)
    }
}

#[cfg(test)]
#[path = "array_elems_tests.rs"]
mod array_elems_tests;
