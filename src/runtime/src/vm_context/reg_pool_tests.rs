use super::*;

#[test]
fn take_is_all_null_even_after_a_used_file_was_returned() {
    let pool = RegPool::default();
    let mut regs = pool.take(4);
    regs[0] = Value::I64(7);
    regs[3] = Value::Bool(true);
    let cap = regs.capacity();
    pool.give(regs);

    let again = pool.take(6);
    assert!(again.capacity() >= cap);
    assert_eq!(again.len(), 6);
    assert!(again.iter().all(|v| matches!(v, Value::Null)));
}

#[test]
fn take_shrinks_to_the_requested_length() {
    let pool = RegPool::default();
    pool.give(pool.take(16));
    assert_eq!(pool.take(2).len(), 2);
}

#[test]
fn pool_is_bounded() {
    let pool = RegPool::default();
    for _ in 0..(CAP + 10) {
        pool.give(vec![Value::Null; 1]);
    }
    // SAFETY: single-threaded test.
    assert_eq!(unsafe { &*pool.free.get() }.len(), CAP);
}
