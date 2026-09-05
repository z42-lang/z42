//! GC cycle-collection benchmarks — add-mark-sweep-collector P4 (2026-05-21).
//!
//! Three workloads exercise distinct shapes of the cycle collector:
//!
//! 1. **cycle_heavy_100** — 100 disjoint 2-node cycles, no roots. Stresses
//!    "many small cycles to reclaim" — the case where trial-deletion's
//!    O(N²) tentative-count bookkeeping was the worst against mark-sweep.
//!
//! 2. **shallow_tree_1k** — single pinned root → 1000-node tree (10-wide,
//!    3-deep object graph), nothing to reclaim. Stresses pure mark-phase
//!    BFS throughput.
//!
//! 3. **large_array_10k** — 1 pinned array with 10k object elements, all
//!    survivors. Stresses single-large-payload mark + Vec iteration.
//!
//! Bench shape: `iter_batched` separates the heap setup (cycle / tree
//! build) from the timed phase (`force_collect`). Reported numbers are
//! collect-cycle wall time only.
//!
//! Run via: `cargo bench --bench gc_cycle_bench`.

use std::sync::Arc;

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, Criterion,
};

use z42::gc::{ArcMagrGC, GcMode, MagrGC};
use z42::metadata::{tokens::TypeId, NativeData, TypeDesc, Value};
use z42::vm_context::VmContext;

fn make_type_desc(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        name: name.to_string(),
        base_name: None,
        class_flags: 0,
        visibility: 0,
        fields: Vec::new(),
        field_index: z42::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: z42::metadata::NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    })
}

fn build_cycle_heavy(heap: &dyn MagrGC, num_cycles: usize) {
    // unify-object-byte-layout (PR-2/PR-3): `ScriptObject` has no `slots`; a field is
    // written via `set_field_value` against the type's composed layout. Give `Cycle` one
    // reference field (slot 0, side-table) so `a ⇄ b` forms a real traced cycle.
    let td = {
        use z42::metadata::types::{ObjectLayout, FieldAccess, TypeDescCold, STRUCT_REF_GCREF, TAG_OBJECT};
        let layout = Arc::new(ObjectLayout {
            size: 8,
            field_offsets: Box::new([0]),
            field_sizes:   Box::new([8]),
            field_kinds:   Box::new([2]), // StructLeafKind::GcRef
            ref_offsets:   Box::new([0]),
            ref_kinds:     Box::new([STRUCT_REF_GCREF]),
            inline_refs:   Box::new([]),
            field_access:  Box::new([FieldAccess { offset: 0, width: 8, tag: TAG_OBJECT, ref_slot: 0 }]),
        });
        Arc::new(TypeDesc {
            name: "Cycle".to_string(),
            base_name: None,
            class_flags: 0,
            visibility: 0,
            fields: Vec::new(),
            field_index: z42::metadata::NameIndex::new(),
            vtable: Vec::new(),
            vtable_index: z42::metadata::NameIndex::new(),
            cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(layout), ..Default::default() })),
            id: TypeId::UNRESOLVED,
        })
    };
    for _ in 0..num_cycles {
        let a = heap.alloc_object(td.clone(), vec![Value::Null], NativeData::None);
        let b = heap.alloc_object(td.clone(), vec![Value::Null], NativeData::None);
        {
            let Value::Object(a_gc) = &a else { unreachable!() };
            let Value::Object(b_gc) = &b else { unreachable!() };
            a_gc.borrow_mut().set_field_value(0, &b);
            b_gc.borrow_mut().set_field_value(0, &a);
        }
        // a / b drop at scope end → cycle becomes unrooted.
    }
}

fn build_shallow_tree(heap: &dyn MagrGC) -> Value {
    // 1 root → 10 children → each 10 grandchildren → each 10 great-grandchildren
    // Total = 1 + 10 + 100 + 1000 = 1111 nodes.
    let td = make_type_desc("Node");
    let leaves: Vec<Value> = (0..1000)
        .map(|_| heap.alloc_object(td.clone(), vec![Value::Null], NativeData::None))
        .collect();
    let mid: Vec<Value> = (0..100)
        .map(|i| {
            let kids: Vec<Value> = leaves[i * 10..(i + 1) * 10].to_vec();
            let kids_arr = heap.alloc_array(kids);
            heap.alloc_object(td.clone(), vec![kids_arr], NativeData::None)
        })
        .collect();
    let top: Vec<Value> = (0..10)
        .map(|i| {
            let kids: Vec<Value> = mid[i * 10..(i + 1) * 10].to_vec();
            let kids_arr = heap.alloc_array(kids);
            heap.alloc_object(td.clone(), vec![kids_arr], NativeData::None)
        })
        .collect();
    let top_arr = heap.alloc_array(top);
    heap.alloc_object(td, vec![top_arr], NativeData::None)
}

fn build_large_array(heap: &dyn MagrGC, n: usize) -> Value {
    let td = make_type_desc("Elem");
    let elems: Vec<Value> = (0..n)
        .map(|_| heap.alloc_object(td.clone(), vec![Value::Null], NativeData::None))
        .collect();
    heap.alloc_array(elems)
}

fn bench_cycle_heavy_100(c: &mut Criterion) {
    c.bench_function("gc_cycle/cycle_heavy_100", |b| {
        b.iter_batched(
            || {
                let heap = ArcMagrGC::new();
                heap.pause(); // disable auto-collect during setup
                build_cycle_heavy(&heap, 100);
                heap.resume();
                heap
            },
            |heap| {
                let stats = heap.force_collect();
                black_box(stats);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_shallow_tree_1k(c: &mut Criterion) {
    c.bench_function("gc_cycle/shallow_tree_1k", |b| {
        b.iter_batched(
            || {
                let heap = ArcMagrGC::new();
                heap.pause();
                let root = build_shallow_tree(&heap);
                let _pin = heap.pin_root(root);
                heap.resume();
                heap
            },
            |heap| {
                let stats = heap.force_collect();
                black_box(stats);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_large_array_10k(c: &mut Criterion) {
    c.bench_function("gc_cycle/large_array_10k", |b| {
        b.iter_batched(
            || {
                let heap = ArcMagrGC::new();
                heap.pause();
                let arr = build_large_array(&heap, 10_000);
                let _pin = heap.pin_root(arr);
                heap.resume();
                heap
            },
            |heap| {
                let stats = heap.force_collect();
                black_box(stats);
            },
            BatchSize::SmallInput,
        );
    });
}

// ── lighten-criterion-ab (2026-09-05): informational bench 的排除开关 ────────
//
// CI 的 criterion A/B（bench-pr.yml Part C）里，`concurrent_*` 三条**从不判红**
// —— 判红脚本对含 "concurrent" 的条目只打印不 fail：多线程 GC 在共享 runner 上
// 的**跑间**线程调度噪声，criterion 的 within-run CI 完全没建模（实测一个
// perf-neutral PR 上它们摆动 +6~33%，而单线程那些围绕 0）。但它们照样占掉每个
// 碰 `src/runtime` 的 PR **75s**（criterion 步骤 531s 的 14%）。CI 因此设
// `Z42_BENCH_SKIP_INFORMATIONAL=1` 把它们排除在测量之外；本地 `cargo bench`
// 不设这个变量，照常全跑。
//
// **是「排除」不是「白名单」**（同 bench-pr.yml 的改动面守卫）：新加的 bench 默认
// 进门禁，只有在这里显式标成 informational 的才可能被跳过 —— 最坏错向「多跑一条」
// （费时间不费正确性），不会错向「静默少跑」（门禁变瞎且不报错）。
// 跳过时**打印一行**，绝不静默（同 e2e 的 `--tier` 跳过语义）。
fn skip_informational(id: &str) -> bool {
    if std::env::var_os("Z42_BENCH_SKIP_INFORMATIONAL").is_none() {
        return false;
    }
    println!("  \u{21b3} skipped {id} (informational; Z42_BENCH_SKIP_INFORMATIONAL=1)");
    true
}

// ── add-concurrent-gc P6 (2026-05-22): ConcurrentMarkSweep variants ────────
//
// Mirror of the 3 STW benches above but routes through VmContext +
// `collect_cycles_with_context` so the concurrent multi-phase flow runs
// (snapshot → yield → drain → handshake → sweep). Wall-clock total is
// what's measured; this includes the brief STW periods + the concurrent
// mark window. A real "mutator throughput" comparison would need a
// separate harness — for now the wall-clock anchor lets us see if the
// concurrent flow's overhead is in the right order of magnitude.

fn bench_cycle_heavy_100_concurrent(c: &mut Criterion) {
    const ID: &str = "gc_cycle/concurrent_cycle_heavy_100";
    if skip_informational(ID) {
        return;
    }
    c.bench_function(ID, |b| {
        b.iter_batched(
            || {
                let ctx = VmContext::new();
                ctx.heap().set_mode(GcMode::ConcurrentMarkSweep);
                ctx.heap().pause();
                build_cycle_heavy(ctx.heap(), 100);
                ctx.heap().resume();
                ctx
            },
            |ctx| {
                ctx.heap().collect_cycles_with_context(&ctx);
                black_box(&ctx);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_shallow_tree_1k_concurrent(c: &mut Criterion) {
    const ID: &str = "gc_cycle/concurrent_shallow_tree_1k";
    if skip_informational(ID) {
        return;
    }
    c.bench_function(ID, |b| {
        b.iter_batched(
            || {
                let ctx = VmContext::new();
                ctx.heap().set_mode(GcMode::ConcurrentMarkSweep);
                ctx.heap().pause();
                let root = build_shallow_tree(ctx.heap());
                let _pin = ctx.heap().pin_root(root);
                ctx.heap().resume();
                ctx
            },
            |ctx| {
                ctx.heap().collect_cycles_with_context(&ctx);
                black_box(&ctx);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_large_array_10k_concurrent(c: &mut Criterion) {
    const ID: &str = "gc_cycle/concurrent_large_array_10k";
    if skip_informational(ID) {
        return;
    }
    c.bench_function(ID, |b| {
        b.iter_batched(
            || {
                let ctx = VmContext::new();
                ctx.heap().set_mode(GcMode::ConcurrentMarkSweep);
                ctx.heap().pause();
                let arr = build_large_array(ctx.heap(), 10_000);
                let _pin = ctx.heap().pin_root(arr);
                ctx.heap().resume();
                ctx
            },
            |ctx| {
                ctx.heap().collect_cycles_with_context(&ctx);
                black_box(&ctx);
            },
            BatchSize::SmallInput,
        );
    });
}

// ── add-custom-allocator P3 (2026-05-22): alloc + sweep throughput ──────────
//
// Measure the constant-factor improvements the region allocator delivers
// on the alloc hot path + sweep traversal. Pre-spec baseline is captured
// via `git worktree` at the commit before P0 (30509787 spec-only); same
// bench file is copied there and run for comparison.

fn bench_alloc_throughput_10k_objects(c: &mut Criterion) {
    c.bench_function("gc_alloc/object_throughput_10k", |b| {
        b.iter_batched(
            || ArcMagrGC::new(),
            |heap| {
                heap.pause();  // disable auto-collect during the loop
                for _ in 0..10_000 {
                    let _ = black_box(heap.alloc_object(
                        make_type_desc("AllocBench"),
                        vec![],
                        NativeData::None,
                    ));
                }
                heap.resume();
                black_box(heap);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_alloc_throughput_10k_arrays(c: &mut Criterion) {
    c.bench_function("gc_alloc/array_throughput_10k", |b| {
        b.iter_batched(
            || ArcMagrGC::new(),
            |heap| {
                heap.pause();
                for _ in 0..10_000 {
                    let _ = black_box(heap.alloc_array(vec![Value::I64(0); 4]));
                }
                heap.resume();
                black_box(heap);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_sweep_10k_survivors(c: &mut Criterion) {
    // 10k pinned objects → sweep visits every entry, all survive → mostly
    // measures the per-entry "is_marked → clear_mark" loop cost.
    c.bench_function("gc_sweep/10k_survivors", |b| {
        b.iter_batched(
            || {
                let heap = ArcMagrGC::new();
                heap.pause();
                let mut pins = Vec::with_capacity(10_000);
                for _ in 0..10_000 {
                    let v = heap.alloc_object(
                        make_type_desc("Survivor"),
                        vec![],
                        NativeData::None,
                    );
                    pins.push(heap.pin_root(v));
                }
                heap.resume();
                (heap, pins)
            },
            |(heap, _pins)| {
                let stats = heap.force_collect();
                black_box(stats);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_sweep_10k_garbage(c: &mut Criterion) {
    // 10k unpinned objects → sweep tombstones every entry → measures the
    // unmarked-tombstone path (finalize_now-style overhead × 10k).
    c.bench_function("gc_sweep/10k_garbage", |b| {
        b.iter_batched(
            || {
                let heap = ArcMagrGC::new();
                heap.pause();
                for _ in 0..10_000 {
                    let _ = heap.alloc_object(
                        make_type_desc("Garbage"),
                        vec![],
                        NativeData::None,
                    );
                }
                heap.resume();
                heap
            },
            |heap| {
                let stats = heap.force_collect();
                black_box(stats);
            },
            BatchSize::SmallInput,
        );
    });
}

// ── add-generational-gc P4 (2026-05-22): minor vs major pause time ─────────
//
// Workloads that exhibit generational hypothesis: large pinned old-gen +
// small ephemeral young churn. Compare:
//   - StwMarkSweep (baseline): every collect = full O(reachable) scan
//   - GenerationalMarkSweep: minor = O(young + dirty cards) ≪ full

fn bench_minor_only_pure_young_churn(c: &mut Criterion) {
    // 1000 small unrooted young objects → minor sweeps them all.
    // Pure young workload — no old, no cross-gen.
    c.bench_function("gc_minor/pure_young_churn_1k", |b| {
        b.iter_batched(
            || {
                let ctx = VmContext::new();
                ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
                ctx.heap().pause();
                for _ in 0..1000 {
                    let _ = ctx.heap().alloc_object(
                        make_type_desc("Y"), vec![], NativeData::None,
                    );
                }
                ctx.heap().resume();
                ctx
            },
            |ctx| {
                ctx.heap().collect_cycles_with_context(&ctx);
                black_box(&ctx);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_minor_pause_with_large_pinned_old_gen(c: &mut Criterion) {
    // 10k pinned old objects (simulated by repeated minors) + 1k
    // ephemeral young → minor's young-list-only sweep should be much
    // smaller than full STW.
    //
    // Note: setup cost is high (10 minors to promote) but excluded by
    // iter_batched. Steady-state minor runs over 1k young.
    c.bench_function("gc_minor/1k_young_with_10k_pinned_old", |b| {
        b.iter_batched(
            || {
                let ctx = VmContext::new();
                ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
                // Build pinned old gen
                let mut pins = Vec::with_capacity(10_000);
                for _ in 0..10_000 {
                    let v = ctx.heap().alloc_object(
                        make_type_desc("OldPinned"), vec![], NativeData::None,
                    );
                    pins.push(ctx.heap().pin_root(v));
                }
                // Promote to old gen.
                for _ in 0..2 {
                    ctx.heap().force_collect();
                }
                // Now add 1k young (unrooted = ephemeral).
                ctx.heap().pause();
                for _ in 0..1000 {
                    let _ = ctx.heap().alloc_object(
                        make_type_desc("Y"), vec![], NativeData::None,
                    );
                }
                ctx.heap().resume();
                (ctx, pins)
            },
            |(ctx, _pins)| {
                ctx.heap().collect_cycles_with_context(&ctx);
                black_box(&ctx);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_full_collect_with_large_pinned_old_baseline(c: &mut Criterion) {
    // Same workload but under StwMarkSweep. Every collect is full
    // O(reachable) — the baseline that generational is supposed to beat.
    c.bench_function("gc_minor/baseline_full_collect_10k_old_1k_young", |b| {
        b.iter_batched(
            || {
                let ctx = VmContext::new();
                // Default StwMarkSweep.
                let mut pins = Vec::with_capacity(10_000);
                for _ in 0..10_000 {
                    let v = ctx.heap().alloc_object(
                        make_type_desc("OldPinned"), vec![], NativeData::None,
                    );
                    pins.push(ctx.heap().pin_root(v));
                }
                ctx.heap().force_collect();
                ctx.heap().pause();
                for _ in 0..1000 {
                    let _ = ctx.heap().alloc_object(
                        make_type_desc("Y"), vec![], NativeData::None,
                    );
                }
                ctx.heap().resume();
                (ctx, pins)
            },
            |(ctx, _pins)| {
                ctx.heap().collect_cycles_with_context(&ctx);
                black_box(&ctx);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_cycle_heavy_100,
    bench_shallow_tree_1k,
    bench_large_array_10k,
    bench_cycle_heavy_100_concurrent,
    bench_shallow_tree_1k_concurrent,
    bench_large_array_10k_concurrent,
    bench_alloc_throughput_10k_objects,
    bench_alloc_throughput_10k_arrays,
    bench_sweep_10k_survivors,
    bench_sweep_10k_garbage,
    bench_minor_only_pure_young_churn,
    bench_minor_pause_with_large_pinned_old_gen,
    bench_full_collect_with_large_pinned_old_baseline,
);
criterion_main!(benches);
