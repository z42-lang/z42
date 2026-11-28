//! Whole-value access to inline heap union bytes.
use super::{Cells, Value, ref_word};
use crate::zpkg::ir::{UnionLayoutDesc, UnionLayoutEntry, UnionLayoutNode};
use anyhow::{ensure, Result};
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;

const LOCK_COUNT: usize = 64;
static LOCKS: [Mutex<()>; LOCK_COUNT] = [const { Mutex::new(()) }; LOCK_COUNT];

pub(crate) fn is_storage_name(name: &str) -> bool {
    name.starts_with("$union.")
}

pub(crate) fn read(cells: &Cells, offset: usize, bytes: &mut [u8]) -> Result<()> {
    let _guard = lock(cells, offset, bytes.len());
    ensure!(cells.copy_out(offset, bytes), "inline union read exceeds storage");
    Ok(())
}

pub(crate) fn write(cells: &Cells, offset: usize, bytes: &[u8]) -> Result<()> {
    let _guard = lock(cells, offset, bytes.len());
    ensure!(cells.copy_in(offset, bytes), "inline union write exceeds storage");
    Ok(())
}

pub(crate) fn lock(cells: &Cells, offset: usize, size: usize) -> Option<MutexGuard<'static, ()>> {
    // Contiguous equal-width single-cell accesses are already atomic. All other
    // accesses to the same union use the same shard, regardless of its owner.
    if size <= 8 && size.is_power_of_two() && offset % size == 0 {
        return None;
    }
    let address = cells.base_ptr() as usize + offset;
    Some(LOCKS[((address >> 3) ^ (address >> 12)) % LOCK_COUNT].lock())
}

pub(crate) fn snapshot(
    cells: &Cells, base: usize, layout: &UnionLayoutDesc, bytes: &mut [u8], refs: &mut [Value],
) -> Result<()> {
    let root = layout.root();
    ensure!(bytes.len() == root.size as usize && refs.len() == root.reference_count as usize,
        "inline union snapshot shape mismatch");
    let _guard = lock(cells, base, bytes.len());
    ensure!(cells.copy_out(base, bytes), "inline union snapshot exceeds storage");
    refs.fill(Value::Null);
    walk(layout, |offset, width| tag_in_cells(cells, base + offset, width), |offset, slot| {
        refs[slot] = ref_word::load(cells, base + offset);
        bytes[offset..offset + 8].fill(0);
    })
}

/// Replace an image whose active reference slots have already been encoded.
pub(crate) fn replace_encoded(
    cells: &Cells, base: usize, layout: &UnionLayoutDesc, bytes: &[u8],
) -> Result<()> {
    ensure!(bytes.len() == layout.root().size as usize
        && base.checked_add(bytes.len()).is_some_and(|end| end <= cells.len()),
        "inline union write shape mismatch");
    let _guard = lock(cells, base, bytes.len());
    walk(layout, |offset, width| tag_in_cells(cells, base + offset, width), |offset, _| {
        ref_word::store_word_recorded(cells, base + offset, 0);
    })?;
    ensure!(cells.copy_in(base, bytes), "inline union write exceeds storage");
    Ok(())
}

pub(crate) fn clear(cells: &Cells, base: usize, layout: &UnionLayoutDesc) -> Result<()> {
    let size = layout.root().size as usize;
    let _guard = lock(cells, base, size);
    walk(layout, |offset, width| tag_in_cells(cells, base + offset, width), |offset, _| {
        ref_word::store_word_recorded(cells, base + offset, 0);
    })?;
    ensure!(cells.clear_prims(base, size, 1), "inline union clear exceeds storage");
    Ok(())
}

pub(crate) fn visit_refs(
    cells: &Cells, base: usize, layout: &UnionLayoutDesc, visit: &mut dyn FnMut(usize, &Value),
) -> Result<()> {
    let mut values = SmallVec::<[(usize, Value); 4]>::new();
    {
        let _guard = lock(cells, base, layout.root().size as usize);
        walk(layout, |offset, width| tag_in_cells(cells, base + offset, width), |offset, _| {
            ref_word::visit(cells, base + offset, &mut |value| values.push((base + offset, *value)));
        })?;
    }
    for (offset, value) in &values { visit(*offset, value); }
    Ok(())
}

pub(crate) fn clear_for_sweep(cells: &Cells, base: usize, layout: &UnionLayoutDesc) -> Result<()> {
    let _guard = lock(cells, base, layout.root().size as usize);
    walk(layout, |offset, width| tag_in_cells(cells, base + offset, width), |offset, _| {
        ref_word::clear(cells, base + offset);
    })
}

pub(crate) fn visit_slots(
    cells: &Cells, base: usize, layout: &UnionLayoutDesc, slots: &mut [Value],
) -> Result<()> {
    let _guard = lock(cells, base, layout.root().size as usize);
    walk(layout, |offset, width| tag_in_cells(cells, base + offset, width), |offset, slot| {
        let word = cells.load_ref_word(base + offset);
        // SAFETY: walk selects only active reference words of a live owner.
        slots[slot] = unsafe { ref_word::decode_for_trace(word) };
    })
}

fn tag_in_cells(cells: &Cells, offset: usize, width: u8) -> Option<usize> {
    cells.load_bits(offset, width as usize).map(|tag| tag as usize)
}

pub(crate) fn tag_in_bytes(bytes: &[u8], offset: usize, width: u8) -> Option<usize> {
    let end = offset.checked_add(width as usize)?;
    let mut tag = [0; 4];
    tag[..width as usize].copy_from_slice(bytes.get(offset..end)?);
    Some(u32::from_le_bytes(tag) as usize)
}

/// Visits physical offsets and frame reference slots for the active cases.
/// An explicit stack also handles deep, validated layouts without recursion.
pub(crate) fn walk(
    layout: &UnionLayoutDesc,
    mut tag: impl FnMut(usize, u8) -> Option<usize>,
    mut visit: impl FnMut(usize, usize),
) -> Result<()> {
    fn active<'a>(
        tag: &mut impl FnMut(usize, u8) -> Option<usize>, base: usize, node: &'a UnionLayoutNode,
    ) -> Result<&'a [UnionLayoutEntry]> {
        let value = tag(base, node.tag_bytes)
            .ok_or_else(|| anyhow::anyhow!("union tag exceeds storage"))?;
        node.cases.get(value).map(Box::as_ref)
            .ok_or_else(|| anyhow::anyhow!("invalid union tag {value}"))
    }

    let root = active(&mut tag, 0, layout.root())?;
    let mut pending: SmallVec<[(usize, usize, std::slice::Iter<'_, UnionLayoutEntry>); 8]> = SmallVec::new();
    pending.push((0, 0, root.iter()));
    while let Some((base, slots, entries)) = pending.last_mut() {
        let Some(entry) = entries.next() else { pending.pop(); continue };
        let offset = *base + entry.offset as usize;
        let slot = *slots + entry.slot as usize;
        if let Some(child) = entry.child {
            let entries = active(&mut tag, offset, &layout.nodes[child as usize])?;
            pending.push((offset, slot, entries.iter()));
        } else {
            visit(offset, slot);
        }
    }
    Ok(())
}
