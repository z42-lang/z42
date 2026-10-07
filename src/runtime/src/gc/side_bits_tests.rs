use super::*;

#[test]
fn set_clear_report_the_transition() {
    let mut w = [0u64; 4];
    assert!(set(&mut w, 70));
    assert!(!set(&mut w, 70), "setting twice is not a transition");
    assert!(test(&w, 70));
    assert_eq!(count(&w), 1);
    assert!(clear(&mut w, 70));
    assert!(!clear(&mut w, 70));
    assert!(is_empty(&w));
}

#[test]
fn for_each_visits_lowest_first_and_first_agrees() {
    let mut w = [0u64; 4];
    for i in [255, 0, 64, 63, 130] {
        set(&mut w, i);
    }
    let mut seen = Vec::new();
    for_each(&w, |i| seen.push(i));
    assert_eq!(seen, vec![0, 63, 64, 130, 255]);
    assert_eq!(first(&w), Some(0));
    assert_eq!(first(&[0u64; 4]), None);
}

#[test]
fn prefix_covers_exactly_n_bits() {
    for n in [0usize, 1, 63, 64, 65, 128, 200, 256] {
        let p = prefix::<4>(n);
        assert_eq!(count(&p), n, "prefix({n})");
        if n > 0 {
            assert!(test(&p, n - 1));
        }
        if n < 256 {
            assert!(!test(&p, n));
        }
    }
}
