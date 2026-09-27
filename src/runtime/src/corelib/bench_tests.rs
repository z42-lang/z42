use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> { VmContext::new() }

#[test]
fn now_ns_is_monotonic_non_decreasing() {
    let a = match builtin_time_now_mono_ns(&ctx(), &[]).unwrap() {
        Value::I64(n) => n,
        other => panic!("expected I64, got {:?}", other),
    };
    let b = match builtin_time_now_mono_ns(&ctx(), &[]).unwrap() {
        Value::I64(n) => n,
        other => panic!("expected I64, got {:?}", other),
    };
    assert!(b >= a, "expected monotonic time, but got a={} b={}", a, b);
    // Both should be non-negative since EPOCH initialises to Instant::now()
    // on the first call within this test process.
    assert!(a >= 0);
}

#[test]
fn now_ns_advances_across_busy_loop() {
    let a = match builtin_time_now_mono_ns(&ctx(), &[]).unwrap() {
        Value::I64(n) => n,
        _ => unreachable!(),
    };
    // Trivial busy work to make sure the second sample is strictly later in
    // wall time even on extremely fast hosts. Don't sleep — flaky on CI.
    let mut sum: u64 = 0;
    for i in 0..100_000u64 { sum = sum.wrapping_add(i); }
    std::hint::black_box(sum);
    let b = match builtin_time_now_mono_ns(&ctx(), &[]).unwrap() {
        Value::I64(n) => n,
        _ => unreachable!(),
    };
    assert!(b > a, "expected b > a after busy loop, got a={} b={}", a, b);
}

#[test]
fn black_box_returns_arg_unchanged_int() {
    let r = builtin_bench_black_box(&ctx(), &[Value::I64(42)]).unwrap();
    assert_eq!(r, Value::I64(42));
}

#[test]
fn black_box_returns_arg_unchanged_bool() {
    let r = builtin_bench_black_box(&ctx(), &[Value::Bool(true)]).unwrap();
    assert_eq!(r, Value::Bool(true));
}

#[test]
fn black_box_returns_arg_unchanged_string() {
    let r = builtin_bench_black_box(&ctx(), &[Value::Str("xyz".into())]).unwrap();
    assert_eq!(r, Value::Str("xyz".into()));
}

/// split-null-sentinel-channels ⑥（2026-09-28）：**arity 不符报错，不再静默给 Null**。
///
/// 🔴 本测试原名 `black_box_no_arg_returns_null`，断言的是「零参调用返回 `Value::Null`」
/// —— 那正是本 change 要消掉的混用：「少传了参数」与「显式传了 null」压成同一个值。
/// `Bench.BlackBox` 的 stdlib 声明是 1 个形参，零参只可能来自 stdlib 声明与 Rust 实现
/// 不一致（编译器会校验 extern 调用点）⇒ 那是维护错误，该报出来而不是吞掉。
#[test]
fn black_box_arity_mismatch_raises() {
    let err = builtin_bench_black_box(&ctx(), &[]).unwrap_err().to_string();
    assert!(err.contains("expected 1 argument(s), got 0"), "实际消息：{err}");
    // 回归门：正常一参照旧原样返回（不是「把整条路堵死」）。
    let r = builtin_bench_black_box(&ctx(), &[Value::I64(7)]).unwrap();
    assert_eq!(r, Value::I64(7));
}
