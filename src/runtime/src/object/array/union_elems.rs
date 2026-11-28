//! Whole-value access to inline union array elements.
use crate::object::{ArrayObj, Cells, StructTypeLayout, Value};
use crate::object::value::union_blob;
use anyhow::{ensure, Result};

impl ArrayObj {
    pub(crate) fn inline_union_layout(&self) -> Option<&StructTypeLayout> {
        self.struct_layout_ref().filter(|layout| layout.union_layout.is_some())
    }

    pub(crate) fn read_inline_union(&self, index: usize, bytes: &mut [u8], refs: &mut [Value]) -> Result<()> {
        let (cells, offset, layout) = self.union_cells(index, bytes.len())?;
        if let Some(plan) = layout.union_layout.as_deref().filter(|_| layout.ref_count() != 0) {
            return union_blob::snapshot(&cells, offset, plan, bytes, refs);
        }
        ensure!(refs.is_empty(), "reference slots for a reference-free union");
        union_blob::read(&cells, offset, bytes)
    }

    pub(crate) fn write_inline_union_encoded(&self, index: usize, bytes: &[u8]) -> Result<()> {
        let (cells, offset, layout) = self.union_cells(index, bytes.len())?;
        if let Some(plan) = layout.union_layout.as_deref().filter(|_| layout.ref_count() != 0) {
            return union_blob::replace_encoded(&cells, offset, plan, bytes);
        }
        union_blob::write(&cells, offset, bytes)
    }

    pub(super) fn clear_inline_unions(&self, index: usize, length: usize, layout: &StructTypeLayout) -> Result<()> {
        let cells = self.cells();
        let size = layout.size;
        ensure!(length <= self.len() && index <= self.len() - length, "invalid inline union clear range");
        if let Some(plan) = layout.union_layout.as_deref().filter(|_| layout.ref_count() != 0) {
            for element in index..index + length {
                union_blob::clear(&cells, element * size, plan)?;
            }
            return Ok(());
        }
        if size <= 8 && size.is_power_of_two() {
            ensure!(cells.clear_prims(index, length, size), "inline union clear exceeds array storage");
            return Ok(());
        }
        for element in index..index + length {
            let offset = element * size;
            let _guard = union_blob::lock(&cells, offset, size);
            ensure!(cells.clear_prims(offset, size, 1), "inline union clear exceeds array storage");
        }
        Ok(())
    }

    pub(super) fn copy_inline_unions(&self, src: &Self, si: usize, di: usize, n: usize, size: usize) {
        if size <= 8 && size.is_power_of_two() {
            let ok = self.cells().copy_prims(di, &src.cells(), si, n, size);
            debug_assert!(ok, "inline union copy must fit both arrays");
            return;
        }
        let mut inline = [0; 64];
        let mut large;
        let bytes = if size <= inline.len() {
            &mut inline[..size]
        } else {
            large = vec![0; size];
            &mut large[..]
        };
        // Release the source guard before taking the destination guard. A reverse
        // traversal preserves memmove semantics even when both arrays are the same.
        let mut copy = |i| {
            union_blob::read(&src.cells(), (si + i) * size, bytes).expect("checked union copy source");
            self.write_inline_union_encoded(di + i, bytes).expect("checked union copy destination");
        };
        if di > si {
            (0..n).rev().for_each(&mut copy);
        } else {
            (0..n).for_each(&mut copy);
        }
    }

    fn union_cells(&self, index: usize, size: usize) -> Result<(Cells<'_>, usize, &StructTypeLayout)> {
        let layout = self.inline_union_layout().filter(|layout| layout.size == size)
            .ok_or_else(|| anyhow::anyhow!("inline union array element layout mismatch"))?;
        ensure!(index < self.len(), "inline union array index out of bounds");
        Ok((self.cells(), index * size, layout))
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "union_elems_tests.rs"]
mod tests;
