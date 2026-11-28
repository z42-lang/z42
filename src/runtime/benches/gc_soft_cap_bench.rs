//! Measure a full collection after a burst of allocations in a single size class.

use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use z42::gc::{GcConfig, Heap};

const ALLOCATIONS: usize = 400_000;
const SOFT_CAP: u64 = 256 * 1024;

fn burst(tail: &str) -> Heap {
    let heap = Heap::new(GcConfig::default());
    heap.pause();
    for _ in 0..ALLOCATIONS {
        black_box(heap.alloc_str("sixteen-byte-str"));
    }
    // Move the bump cursor to a chunk in a different size class so the small
    // allocations can be reclaimed together.
    black_box(heap.alloc_str(tail));
    black_box(heap.alloc_str(tail));
    heap.resume();
    heap.set_max_heap_bytes(Some(SOFT_CAP));
    heap
}

fn collect_after_burst(c: &mut Criterion) {
    let tail = "x".repeat(48 * 1024);
    let heap = burst(&tail);
    heap.collect_full();
    let stats = heap.stats();
    eprintln!(
        "after full GC: occupied={} committed={} pooled={}",
        stats.occupied_bytes, stats.committed_bytes, stats.pooled_bytes
    );
    drop(heap);

    let mut group = c.benchmark_group("gc_soft_cap");
    group.sample_size(12);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("full_after_burst", |b| {
        b.iter_batched_ref(
            || burst(&tail),
            |heap| black_box(heap.collect_full()),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

criterion_group!(benches, collect_after_burst);
criterion_main!(benches);
