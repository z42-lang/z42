//! Bench helpers — minimal native primitives for `Std.Test.Bencher`.
//!
//! `__time_now_mono_ns` returns nanoseconds since the first call (an internal
//! EPOCH); used as a monotonic clock by `Bencher.iter` to time samples.
//!
//! `__bench_black_box` is the identity function. Interp does no
//! dead-code elimination, so the wrapper is structurally a no-op today.
//! It exists so user code can mark "do not optimise this away" the way
//! criterion / std::hint::black_box does — once the JIT learns to elide
//! pure expressions, this hook is the canonical opt-out.

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
static EPOCH: OnceLock<Instant> = OnceLock::new();

pub fn builtin_time_now_mono_ns(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    Ok(Value::I64(mono_ns()))
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

pub fn builtin_bench_black_box(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(args.first().cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
#[path = "bench_tests.rs"]
mod bench_tests;
