use super::cursor::Cursor;
use crate::ir::{StructLayoutDesc, UnionLayoutDesc, UnionLayoutEntry, UnionLayoutNode};
use anyhow::{ensure, Result};

pub(super) fn read(
    cursor: &mut Cursor<'_>, storage: &StructLayoutDesc,
) -> Result<UnionLayoutDesc> {
    let count = cursor.read_u32()? as usize;
    // Even an empty case requires the node header and one entry count.
    ensure!(count > 0 && count <= cursor.remaining() / 12, "invalid union layout node count");
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        nodes.push(read_node(cursor, &nodes)?);
    }
    let layout = UnionLayoutDesc { nodes: nodes.into_boxed_slice() };
    let root = layout.root();
    ensure!(root.size == storage.size && root.reference_count as usize == storage.ref_offsets.len(),
        "union layout does not match its frame storage");
    Ok(layout)
}

fn read_node(cursor: &mut Cursor<'_>, previous: &[UnionLayoutNode]) -> Result<UnionLayoutNode> {
    let size = cursor.read_u32()?;
    let align = cursor.read_u8()?;
    let tag_bytes = cursor.read_u8()?;
    let count = cursor.read_u32()? as usize;
    ensure!(matches!(align, 1 | 2 | 4 | 8) && matches!(tag_bytes, 1 | 2 | 4)
        && tag_bytes <= align && size >= align as u32 && size % align as u32 == 0,
        "invalid union size, tag width or alignment");
    let tag_limit = 1u64 << (8 * tag_bytes);
    ensure!(count > 0 && count as u64 <= tag_limit && count <= cursor.remaining() / 2,
        "invalid union case count");
    let mut cases = Vec::with_capacity(count);
    let mut reference_count = 0;
    for _ in 0..count {
        let (entries, count) = read_case(cursor, previous, size, align)?;
        cases.push(entries);
        reference_count = reference_count.max(count);
    }
    Ok(UnionLayoutNode { size, align, tag_bytes, reference_count, cases: cases.into_boxed_slice() })
}

fn read_case(
    cursor: &mut Cursor<'_>, previous: &[UnionLayoutNode], size: u32, align: u8,
) -> Result<(Box<[UnionLayoutEntry]>, u16)> {
    let count = cursor.read_u16()? as usize;
    ensure!(count <= cursor.remaining() / 8, "invalid union reference entry count");
    let mut entries = Vec::with_capacity(count);
    let mut end = align as u32;
    let mut slot: u16 = 0;
    for _ in 0..count {
        let offset = cursor.read_u32()?;
        let child = match cursor.read_u32()? {
            u32::MAX => None,
            index => Some(index),
        };
        let (width, alignment, slots) = match child {
            None => (8, 8, 1),
            Some(index) => {
                let node = previous.get(index as usize)
                    .ok_or_else(|| anyhow::anyhow!("union child layout must precede its parent"))?;
                ensure!(node.reference_count != 0, "nested union reference layout has no references");
                (node.size, node.align, node.reference_count)
            }
        };
        ensure!(alignment <= align && offset % alignment as u32 == 0
            && offset >= end && offset.checked_add(width).is_some_and(|end| end <= size),
            "overlapping, misaligned or out-of-bounds union reference entry");
        entries.push(UnionLayoutEntry { offset, slot, child });
        end = offset + width;
        slot = slot.checked_add(slots).ok_or_else(|| anyhow::anyhow!("too many union reference slots"))?;
    }
    Ok((entries.into_boxed_slice(), slot))
}

#[cfg(test)]
#[path = "union_layout_tests.rs"]
mod tests;
