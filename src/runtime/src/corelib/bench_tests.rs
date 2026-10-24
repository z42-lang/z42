use super::*;
use crate::metadata::Value;
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

#[test]
fn black_box_returns_arg_unchanged_int() {
    let r = bench_black_box(Value::I64(42));
    assert_eq!(r, Value::I64(42));
}

#[test]
fn black_box_returns_arg_unchanged_bool() {
    let r = bench_black_box(Value::Bool(true));
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn black_box_returns_arg_unchanged_string() {
    let r = bench_black_box(Value::Str("xyz".into()));
    assert_eq!(r, Value::Str("xyz".into()));
}

/// split-null-sentinel-channels ⑥：**arity 不符报错，不再静默给 Null**。
///
/// 🔴 本测试原名 `black_box_no_arg_returns_null`，断言的是「零参调用返回 `Value::Null`」
/// —— 那正是本 change 要消掉的混用：「少传了参数」与「显式传了 null」压成同一个值。
/// `Bench.BlackBox` 的 stdlib 声明是 1 个形参，零参只可能来自 stdlib 声明与 Rust 实现
/// 不一致（编译器会校验 extern 调用点）⇒ 那是维护错误，该报出来而不是吞掉。
#[test]
fn black_box_arity_mismatch_raises() {
    // typed 化(P1-8 批次 12)后 arity 由 ABI 解包层检查——经 exec_builtin_value
    // 零参调用必须报「expects 1」,不是静默 Null(split-null-sentinel-channels ⑥)。
    let ctx = ctx();
    let err = crate::corelib::exec_builtin_value(&ctx, "__bench_black_box", &[])
        .expect_err("zero-arg call must raise");
    let msg = format!("{err:#}");
    assert!(msg.contains("expects 1") || msg.contains("argument"), "实际消息：{msg}");
    // 回归门：正常一参照旧原样返回（不是「把整条路堵死」）。
    let r = bench_black_box(Value::I64(7));
    assert_eq!(r, Value::I64(7));
}
