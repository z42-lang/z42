#[test]
fn page_size_is_a_power_of_two() {
    let p = super::page_size();
    assert!(p >= 4096 && p.is_power_of_two());
}

#[test]
fn inner_pages_keeps_only_whole_pages() {
    let p = super::page_size();
    assert_eq!(super::inner_pages(1, 3 * p - 1), Some((p, p)));
    assert_eq!(super::inner_pages(p, 3 * p), Some((p, 2 * p)));
    assert_eq!(super::inner_pages(1, 2 * p - 1), None);
}
