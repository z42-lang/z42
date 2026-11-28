use super::*;
use std::sync::Arc;

#[test]
fn inline_union_stride_is_exact_and_copy_and_clear_change_only_the_selected_elements() {
    for size in [1, 2, 3, 4, 8, 12, 24, 72] {
        let layout = Arc::new(StructTypeLayout { size, ..Default::default() });
        let backing = ArrayObj::struct_leaked("$union.Example", 4, layout);
        let array = backing.borrow();
        assert_eq!(array.elem_storage_bytes(), 4 * size);
        let value = vec![0x7a; size];
        array.write_inline_union(1, &value).unwrap();
        array.copy_elems_within(0, 1, 3);
        assert_eq!(array.struct_elem(2).unwrap().0, value);
        assert_eq!(array.struct_elem(3).unwrap().0, vec![0; size]);
        assert!(array.try_clear_elems(2, 1).unwrap());
        assert_eq!(array.struct_elem(2).unwrap().0, vec![0; size]);
    }
}
