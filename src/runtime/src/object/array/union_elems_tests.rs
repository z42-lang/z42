use super::*;
use std::sync::Arc;

#[test]
fn inline_union_stride_is_exact_and_copy_and_clear_change_only_the_selected_elements() {
    for size in [1, 2, 3, 4, 8, 12, 24, 72] {
        let union_layout = Arc::new(crate::zpkg::ir::UnionLayoutDesc {
            nodes: Box::new([crate::zpkg::ir::UnionLayoutNode {
                size: size as u32, align: 1, tag_bytes: 1, reference_count: 0,
                cases: vec![Box::new([]) as Box<[crate::zpkg::ir::UnionLayoutEntry]>].into_boxed_slice(),
            }]),
        });
        let layout = Arc::new(StructTypeLayout { size, union_layout: Some(union_layout), ..Default::default() });
        let backing = ArrayObj::struct_leaked("$union.Example", 4, layout);
        let array = backing.borrow();
        assert_eq!(array.elem_storage_bytes(), 4 * size);
        let value = vec![0x7a; size];
        array.write_inline_union_encoded(1, &value).unwrap();
        array.copy_elems_within(0, 1, 3);
        assert_eq!(array.struct_elem(2).unwrap().0, value);
        assert_eq!(array.struct_elem(3).unwrap().0, vec![0; size]);
        assert!(array.try_clear_elems(2, 1).unwrap());
        assert_eq!(array.struct_elem(2).unwrap().0, vec![0; size]);
    }
}

#[test]
fn a_wide_union_read_matches_one_complete_concurrent_write() {
    const SIZE: usize = 24;
    let layout = Arc::new(StructTypeLayout {
        size: SIZE,
        union_layout: Some(Arc::new(crate::zpkg::ir::UnionLayoutDesc {
            nodes: Box::new([crate::zpkg::ir::UnionLayoutNode {
                size: SIZE as u32,
                align: 8,
                tag_bytes: 1,
                reference_count: 0,
                cases: vec![Box::new([]) as Box<[crate::zpkg::ir::UnionLayoutEntry]>; 2]
                    .into_boxed_slice(),
            }]),
        })),
        ..Default::default()
    });
    let array = ArrayObj::struct_leaked("$union.Test", 1, layout);
    let zero = [0; SIZE];
    let one = [1; SIZE];
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let array = array.borrow();
            for _ in 0..2048 {
                array.write_inline_union_encoded(0, &one).unwrap();
                array.write_inline_union_encoded(0, &zero).unwrap();
            }
        });
        let array = array.borrow();
        let mut value = [0; SIZE];
        for _ in 0..4096 {
            array.read_inline_union(0, &mut value, &mut []).unwrap();
            assert!(value == zero || value == one);
        }
    });
}
