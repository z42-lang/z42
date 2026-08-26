//! Smoke benchmark — proves the criterion harness compiles and runs.
//!
//! These two benchmarks intentionally avoid touching VM internals, so this file
//! stays a pure-Rust criterion sanity check. It is deliberately NOT part of the
//! CI regression gate: `bench-pr.yml`'s criterion A/B step
//! (extend-ab-bench-micro-criterion) gates `gc_cycle_bench` (a real VM benchmark),
//! not this file. See `docs/book/src/dev/benchmarking.md` for the gate semantics.
//!
//! Bench files that landed since / are still planned:
//! - `gc_cycle_bench.rs` — cycle-collect / mark / sweep costs (LANDED; A/B-gated)
//! - `interp_bench.rs`   — interp dispatch loop, call overhead (planned)
//! - `decoder_bench.rs`  — .zbc decoding throughput (planned)

use criterion::{black_box, criterion_group, criterion_main, Criterion};

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
