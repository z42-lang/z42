//! Bench clock — the native primitive behind `Std.Runtime.Clock.MonoNanos` (`Bencher.iter`).
//!
//! `__time_now_mono_ns` returns nanoseconds since the first call (an internal
//! EPOCH); used as a monotonic clock by `Bencher.iter` to time samples.
//!
//! `BenchHelpers.blackBox` needs no native: it is a plain z42 identity marked
//! `[MethodImpl(NoInlining | NoOptimization)]`, which the compiler's optimizer honours.

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
pub fn time_now_mono_ns() -> i64 {
    mono_ns()
}

/// Nanoseconds since an internal epoch (monotonic). fix-wasm-time-builtins: without a
/// clock (wasm32) `pal::time` counts ticks, so the test runner's timing doesn't trap.
fn mono_ns() -> i64 {
    crate::pal::time::monotonic_ns() as i64
}

#[cfg(test)]
#[path = "bench_tests.rs"]
mod bench_tests;

/// bench 计时迁 builtin ABI v2(P1-8 批次 12):PURE。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__time_now_mono_ns", pure)]
    pub fn now_mono_ns() -> i64 { time_now_mono_ns() }
}
