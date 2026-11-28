use super::*;

fn node(output: &mut Vec<u8>, size: u32, branches: &[&[(u32, u32)]]) {
    output.extend(size.to_le_bytes());
    output.extend([8, 1]);
    output.extend((branches.len() as u32).to_le_bytes());
    for branch in branches {
        output.extend((branch.len() as u16).to_le_bytes());
        for &(offset, child) in *branch {
            output.extend(offset.to_le_bytes());
            output.extend(child.to_le_bytes());
        }
    }
}

fn decode(bytes: &[u8], size: u32, refs: &[u32]) -> Result<UnionLayoutDesc> {
    let storage = StructLayoutDesc {
        size,
        ref_offsets: refs.into(),
        ref_kinds: vec![2; refs.len()].into_boxed_slice(),
    };
    read(&mut Cursor::new(bytes), &storage)
}

#[test]
fn nested_layout_counts_its_reserved_slots_in_each_parent_case() {
    let mut bytes = Vec::from(2u32.to_le_bytes());
    node(&mut bytes, 16, &[&[], &[(8, u32::MAX)]]);
    node(&mut bytes, 32, &[&[(8, 0), (24, u32::MAX)], &[]]);
    let layout = decode(&bytes, 32, &[8, 16]).unwrap();
    assert_eq!(layout.root().reference_count, 2);
    assert_eq!(layout.root().cases[0].as_ref(), [
        UnionLayoutEntry { offset: 8, slot: 0, child: Some(0) },
        UnionLayoutEntry { offset: 24, slot: 1, child: None },
    ]);
    assert!(layout.root().cases[1].is_empty());
}

#[test]
fn malformed_layouts_cannot_identify_nonreference_bytes_as_roots() {
    for entries in [
        &[(0, u32::MAX)][..],
        &[(9, u32::MAX)],
        &[(24, u32::MAX)],
        &[(8, u32::MAX), (8, u32::MAX)],
        &[(8, 0)],
    ] {
        let mut bytes = Vec::from(1u32.to_le_bytes());
        node(&mut bytes, 24, &[entries]);
        assert!(decode(&bytes, 24, &[8]).is_err(), "{entries:?}");
    }
}

#[test]
fn the_root_must_agree_with_the_frame_storage_shape() {
    let mut bytes = Vec::from(1u32.to_le_bytes());
    node(&mut bytes, 16, &[&[(8, u32::MAX)]]);
    assert!(decode(&bytes, 8, &[8]).is_err());
    assert!(decode(&bytes, 16, &[]).is_err());
    assert!(decode(&bytes[..bytes.len() - 1], 16, &[8]).is_err());
}
