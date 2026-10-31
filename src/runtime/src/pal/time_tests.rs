#[test]
fn monotonic_us_never_goes_back() {
    let a = super::monotonic_us();
    let b = super::monotonic_us();
    assert!(b >= a);
    let (c, d) = (super::monotonic_ns(), super::monotonic_ns());
    assert!(d >= c);
}

#[test]
fn wall_clock_is_after_2020() {
    assert!(super::wall_unix_ms() > 1_577_836_800_000);
}
