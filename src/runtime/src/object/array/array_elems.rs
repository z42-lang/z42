//! Array element encoding and access primitives.

#![allow(unused_imports)]
use crate::object::*;

impl ArrayObj {
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
        match self.mode() {
            ArrayMode::Ref => ref_word::load(&self.cells(), i * REF_STRIDE),
            ArrayMode::Mixed => tparam_cell::load_or_zero(&self.cells(), i * CELL_STRIDE, i * CELL_STRIDE + 8, self.zero()),
            ArrayMode::Frozen => self.frozen_values()[i],
            ArrayMode::Struct => {
                // A struct[] element read as a generic `Value` needs a heap box (value semantics);
                // `get_boxed` cannot allocate. The value paths materialize a `StructRefHeap`
                // handle (`objops::array::array_get`) or box through the heap-aware bulk path.
                debug_assert!(false,
                    "get_boxed on a struct[] array: struct[] element boxing needs a heap-aware path");
                Value::Null
            }
            m => {
                let w = self.width();
                let bits = self.cells().load_bits(i * w, w).unwrap_or(0);
                match m {
                    ArrayMode::Bool => Value::Bool(bits != 0),
                    ArrayMode::Bytes => Value::I64(bits as u8 as i64),
                    ArrayMode::I32 => Value::I64(bits as u32 as i32 as i64),
                    ArrayMode::I64 => Value::I64(bits as i64),
                    ArrayMode::Chars => Value::Char(char::from_u32(bits as u32).unwrap_or('\0')),
                    _ => Value::F64(f64::from_bits(bits)),
                }
            }
        }
    }

    /// 基元数组的**种类标签**（`None` = 非基元数组）。标签文本同时用于错误消息。
    #[inline]
    pub fn prim_backing_kind(&self) -> Option<&'static str> {
        Some(match self.mode() {
            ArrayMode::Bool => "bool[]",
            ArrayMode::Bytes => "byte[]",
            ArrayMode::I32 => "int[]",
            ArrayMode::I64 => "long[]",
            ArrayMode::Chars => "char[]",
            ArrayMode::F64 => "double[]",
            _ => return None,
        })
    }

    /// 「这个 `Value` 能**原样**存进本数组吗」——**严格同种**，不做拓宽（`double[]` 不收 `I64`）。
    /// 🔴 这是该问题的唯一判据，`store_elem` 的基元臂必须与它一致
    /// （`prim_backing_accepts_agrees_with_set_boxed` 逐格钉住）。
    /// ⚠️ `int[]` 收 `I64` 是正常路径：IR 整数一律 i64，存入时按 `as i32` 截断。
    #[inline]
    pub fn prim_backing_accepts(&self, val: &Value) -> bool {
        match self.mode() {
            ArrayMode::Bool => matches!(val, Value::Bool(_)),
            ArrayMode::Bytes | ArrayMode::I32 | ArrayMode::I64 => matches!(val, Value::I64(_)),
            ArrayMode::Chars => matches!(val, Value::Char(_)),
            ArrayMode::F64 => matches!(val, Value::F64(_)),
            _ => true,
        }
    }

    /// The raw bits a PRIM array stores for `val` (a mismatch stores zero after a debug assert —
    /// [`prim_value_mismatch`]).
    #[inline(always)]
    pub(in crate::object) fn prim_bits(&self, val: &Value, site: &str) -> u64 {
        match (self.mode(), val) {
            (ArrayMode::Bool, Value::Bool(x)) => *x as u64,
            (ArrayMode::Bytes | ArrayMode::I32 | ArrayMode::I64, Value::I64(n)) => *n as u64,
            (ArrayMode::Chars, Value::Char(c)) => *c as u64,
            (ArrayMode::F64, Value::F64(f)) => f.to_bits(),
            _ => {
                prim_value_mismatch(val, self.prim_backing_kind().unwrap_or("?"), site);
                0
            }
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
    /// element store reports `NotRef`; its caller barriers the element's leaves.
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

    /// The element store behind [`Self::set_boxed`] / [`Self::try_set_boxed`]. `&self`: an array
    /// is shared between threads without a lock, every store is one atomic cell write (object
    /// model R4). A FROZEN array refuses (written once, before publication).
    pub fn store_elem(&self, i: usize, val: &Value) -> anyhow::Result<FieldWrite> {
        match self.mode() {
            ArrayMode::Ref => ref_word::store(&self.cells(), i * REF_STRIDE, val),
            ArrayMode::Mixed => tparam_cell::store_or_zero(&self.cells(), i * CELL_STRIDE, i * CELL_STRIDE + 8, val, self.zero()),
            // A 16 B `Value` cannot be stored atomically: FROZEN blocks are written once, before
            // the array is published (closure environments, `ref_word` kind-7 boxes), and are
            // read-only afterwards.
            ArrayMode::Frozen => anyhow::bail!("cannot store into a frozen array (closure environment / value box)"),
            ArrayMode::Struct => self.store_struct_elem_boxed(i, val),
            _ => {
                let bits = self.prim_bits(val, "set_boxed");
                let w = self.width();
                self.cells().store_bits(i * w, w, bits);
                Ok(FieldWrite::NotRef)
            }
        }
    }
}

// ── STRUCT：struct[] 元素（整块 blob 与单个叶子）──
impl ArrayObj {
    /// Atomic loads over a `struct[]` element area (`None` for other modes).
    #[inline]
    pub(in crate::object) fn struct_cells(&self) -> Option<(Cells<'_>, &StructTypeLayout)> {
        Some((self.cells(), self.struct_layout_ref()?))
    }

    /// Element `i` of a `struct[]` in the frame-arena form: its bytes (reference slots zeroed)
    /// and its reference leaves in `ref_offsets` order. `None` for any other mode. Caller
    /// bounds-checks `i`.
    pub fn struct_elem(&self, i: usize) -> Option<(Vec<u8>, Vec<Value>)> {
        let (c, l) = self.struct_cells()?;
        Some(struct_blob::read(&c, i * l.size, l.size, &l.ref_offsets))
    }

    /// Copy an arena-form struct `(bytes, refs)` into element `i` of a `struct[]` (no-op on any
    /// other mode). Every reference that landed goes to `barrier`.
    pub fn write_struct_elem(
        &self, i: usize, src_bytes: &[u8], src_refs: &[Value], barrier: &mut dyn FnMut(&Value),
    ) -> anyhow::Result<()> {
        if let Some((c, l)) = self.struct_cells() {
            struct_blob::write(&c, i * l.size, l.size, src_bytes, src_refs, &l.ref_offsets, barrier)?;
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
        let Some((c, l)) = self.struct_cells() else {
            anyhow::bail!("struct[] leaf write on a non-value-struct array");
        };
        if l.ref_index(off).is_none() || i >= self.len() {
            anyhow::bail!("struct[] ref leaf at byte offset {off} of element {i} not in element layout");
        }
        ref_word::store(&c, i * l.size + off as usize, v)
    }

    /// Read the primitive leaf at blob offset `off` (width `w`, tag `kind`) of element `i`.
    pub fn struct_elem_prim(&self, i: usize, off: u32, w: usize, kind: u8) -> anyhow::Result<Value> {
        let (c, l) = self.struct_cells()
            .ok_or_else(|| anyhow::anyhow!("struct[] leaf read on a non-value-struct array"))?;
        c.load_prim(i * l.size + off as usize, w, kind)
    }

    /// Write the primitive leaf at blob offset `off` of element `i`.
    pub fn set_struct_elem_prim(&self, i: usize, off: u32, w: usize, kind: u8, v: &Value) -> anyhow::Result<()> {
        let Some((c, l)) = self.struct_cells() else {
            anyhow::bail!("struct[] leaf write on a non-value-struct array");
        };
        debug_assert!(l.ref_index(off & !7).is_none(), "primitive store over a struct[] reference leaf");
        c.store_prim(i * l.size + off as usize, w, kind, v)
    }
}

#[cfg(test)]
#[path = "array_elems_tests.rs"]
mod array_elems_tests;
