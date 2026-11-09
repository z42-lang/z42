#[test]
fn fill_produces_bytes() {
    let mut a = [0u8; 32];
    super::fill(&mut a).unwrap();
    let mut b = [0u8; 32];
    super::fill(&mut b).unwrap();
    assert_ne!(a, b);
}
