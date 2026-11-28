//! Translate frame union values into heap images before publication.
use crate::exec::ops::object::struct_leaf::as_struct_ref;
use crate::gc::{Heap, RootHandle};
use crate::object::value::{ref_word, union_blob};
use crate::object::Value;
use crate::vm::VmContext;
use crate::zpkg::ir::UnionLayoutDesc;
use anyhow::{ensure, Result};
use smallvec::SmallVec;

pub(crate) fn with_encoded(
    ctx: &VmContext, value: &Value, expected: &str, size: usize, plan: Option<&UnionLayoutDesc>,
    write: impl FnOnce(&[u8]) -> Result<()>,
) -> Result<()> {
    let reference_count = plan.map_or(0, |plan| plan.root().reference_count as usize);
    if let Value::I64(bits) = value {
        ensure!(size <= 8 && reference_count == 0, "invalid packed union value");
        return write(&bits.to_le_bytes()[..size]);
    }
    let (slot, frame) = as_struct_ref(value, "inline union source")?;
    let check = |source: &crate::exec::call::struct_arena::StructSlot| {
        ensure!(source.type_name.as_ref() == expected && source.bytes.len() == size
            && source.refs.len() == reference_count, "inline union storage type mismatch");
        Ok(())
    };
    let Some(plan) = plan.filter(|_| reference_count != 0) else {
        return ctx.exec().with_struct_slot(slot, frame, |source| {
            check(source)?;
            write(&source.bytes)
        })?;
    };

    let mut bytes = SmallVec::<[u8; 64]>::new();
    let mut active = SmallVec::<[(usize, Value); 4]>::new();
    ctx.exec().with_struct_slot(slot, frame, |source| {
        check(source)?;
        bytes.extend_from_slice(&source.bytes);
        union_blob::walk(plan, |offset, width| union_blob::tag_in_bytes(&bytes, offset, width), |offset, slot| {
            active.push((offset, source.refs[slot]));
        })
    })??;

    // Some references (for example projected interface receivers) use the
    // shared reference-word box. Allocate and root these before taking a heap
    // slot lock or borrowing the frame arena.
    let mut roots = PinnedValues::new(ctx.heap());
    for (offset, value) in active {
        let word = if let Some(word) = ref_word::encode(&value) {
            word
        } else {
            ensure!(matches!(value, Value::Bool(_) | Value::Char(_) | Value::F64(_) | Value::InterfaceRef(_)),
                "invalid inline union reference");
            let boxed = ref_word::alloc_box_ambient(&value)?;
            roots.pin(Value::Array(boxed));
            ref_word::encode_boxed(&boxed)
        };
        bytes[offset..offset + 8].copy_from_slice(&word.to_le_bytes());
    }
    write(&bytes)
}

pub(crate) struct PinnedValues<'a> {
    heap: &'a Heap,
    roots: SmallVec<[RootHandle; 4]>,
}

impl<'a> PinnedValues<'a> {
    pub(crate) fn new(heap: &'a Heap) -> Self {
        Self { heap, roots: SmallVec::new() }
    }

    pub(crate) fn pin(&mut self, value: Value) {
        self.roots.push(self.heap.pin_root(value));
    }
}

impl Drop for PinnedValues<'_> {
    fn drop(&mut self) {
        for handle in self.roots.drain(..) {
            self.heap.unpin_root(handle);
        }
    }
}
