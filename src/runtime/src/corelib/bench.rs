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
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
static EPOCH: OnceLock<Instant> = OnceLock::new();

pub fn time_now_mono_ns() -> i64 {
    mono_ns()
}

/// Nanoseconds since an internal epoch (monotonic).
#[cfg(not(target_arch = "wasm32"))]
fn mono_ns() -> i64 {
    let epoch = EPOCH.get_or_init(Instant::now);
    Instant::now().duration_since(*epoch).as_nanos() as i64
}

/// fix-wasm-time-builtins: `wasm32-unknown-unknown` has no `std::time` —
/// `Instant::now()` panics ("time not implemented on this platform"), which
/// trapped the in-browser embedded test-host (the test runner times each case).
/// Provide a monotonic counter so timing doesn't trap; durations stay
/// non-negative, though the unit is ticks, not real ns. Real mono time on wasm
/// (JS `performance.now`) is Deferred (would need js-sys in this crate).
#[cfg(target_arch = "wasm32")]
fn mono_ns() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static TICK: AtomicI64 = AtomicI64::new(0);
    TICK.fetch_add(1_000, Ordering::Relaxed)
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
