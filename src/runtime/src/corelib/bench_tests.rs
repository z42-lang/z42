use super::*;
use crate::object::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> { VmContext::new() }

#[test]
fn now_ns_is_monotonic_non_decreasing() {
    let a = time_now_mono_ns();
    let b = time_now_mono_ns();
    assert!(b >= a, "expected monotonic time, but got a={} b={}", a, b);
    // Both should be non-negative since EPOCH initialises to Instant::now()
    // on the first call within this test process.
    assert!(a >= 0);
}

#[test]
fn now_ns_advances_across_busy_loop() {
    let a = time_now_mono_ns();
    // Trivial busy work to make sure the second sample is strictly later in
    // wall time even on extremely fast hosts. Don't sleep — flaky on CI.
    let mut sum: u64 = 0;
    for i in 0..100_000u64 { sum = sum.wrapping_add(i); }
    std::hint::black_box(sum);
    let b = time_now_mono_ns();
    assert!(b > a, "expected b > a after busy loop, got a={} b={}", a, b);
}

/// split-null-sentinel-channels ⑥：**arity 不符报错，不再静默给 Null**——typed 化(P1-8 批次 12)后
/// arity 由 ABI 解包层检查，经 exec_builtin_value 多传一个实参必须报错。
#[test]
fn now_ns_arity_mismatch_raises() {
    let ctx = ctx();
    let err = crate::corelib::exec_builtin_value(&ctx, "__time_now_mono_ns", &[Value::I64(1)])
        .expect_err("one-arg call must raise");
    let msg = format!("{err:#}");
    assert!(msg.contains("expects 0") || msg.contains("argument"), "实际消息：{msg}");
}
