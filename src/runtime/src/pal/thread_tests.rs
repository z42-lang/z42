#[test]
fn bounds_contain_a_local() {
    let local = 0u8;
    let here = std::hint::black_box(&local) as *const u8 as usize;
    if let Some((low, high)) = super::current_thread_stack() {
        assert!(
            low < here && here < high,
            "{low:#x} < {here:#x} < {high:#x}"
        );
    }
}

#[test]
fn run_with_stack_returns_the_result() {
    let r = super::run_with_stack("pal-test", 1 << 20, || 6 * 7);
    assert_eq!(r.unwrap(), 42);
}

#[test]
fn run_with_stack_reports_a_panic() {
    let r = super::run_with_stack("pal-test-panic", 1 << 20, || -> i32 { panic!("expected") });
    assert!(matches!(r, Err(super::RunError::Panicked)));
}

#[test]
fn token_is_stable_per_thread_and_differs_across_threads() {
    let here = super::current_token();
    assert_eq!(here, super::current_token());
    let other = std::thread::spawn(super::current_token).join().unwrap();
    assert_ne!(here, other);
}
