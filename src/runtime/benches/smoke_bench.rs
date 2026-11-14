//! Smoke benchmark — proves the criterion harness compiles and runs.
//!
//! These benchmarks avoid VM internals and provide a pure-Rust Criterion sanity check.
//! `xtask bench vm` discovers the declared harness=false Cargo targets, so both this
//! harness and the GC benchmarks participate in the selected VM measurement family.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

/// Pure Rust baseline: tight wrapping_add loop.
/// Acts as a "criterion sanity check" — should be ~ns per iteration.
fn bench_baseline_sum_loop(c: &mut Criterion) {
    c.bench_function("smoke/baseline_sum_loop_1k", |b| {
        b.iter(|| {
            let mut sum: u64 = 0;
            for i in 0..1000_u64 {
                sum = sum.wrapping_add(black_box(i));
            }
            sum
        });
    });
}

/// Pure Rust baseline: string concatenation via push_str.
/// Establishes the cost of a simple allocation-heavy workload.
fn bench_baseline_string_concat(c: &mut Criterion) {
    c.bench_function("smoke/baseline_string_concat_100", |b| {
        b.iter(|| {
            let mut s = String::with_capacity(400);
            for i in 0..100_u32 {
                s.push_str(black_box("item-"));
                s.push_str(&i.to_string());
                s.push(';');
            }
            s
        });
    });
}

criterion_group!(benches, bench_baseline_sum_loop, bench_baseline_string_concat);
criterion_main!(benches);
