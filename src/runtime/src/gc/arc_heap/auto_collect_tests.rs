//! add-gc-runtime-knobs (2026-09-05): auto-collect gating + futility backoff.
//!
//! These drive `maybe_auto_collect` on a bare `ArcMagrGC` (no `VmCore` wiring),
//! so it takes the inline-collect fallback and we can observe `gc_cycles`.

use crate::gc::{ArcMagrGC, MagrGC};

fn cycles(heap: &ArcMagrGC) -> u64 { heap.stats().gc_cycles }

/// **arm-gc-by-default (2026-09-09)** reverses the historical default: with no
/// `Z42_GC_MAX_BYTES` the collector used to *never* run, so a long-lived program grew until
/// it exited. Every threshold is relative now (Mono SGen's shape), so there is nothing left
/// for a budget to switch on.
#[test]
fn no_budget_still_collects() {
    let heap = ArcMagrGC::new();
    heap.set_nursery_bytes_for_test(64 * 1024);
    for _ in 0..4000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    assert!(cycles(&heap) > 0,
        "an unbudgeted heap must still collect (got {} cycles)", cycles(&heap));
}

/// The gate is **relative**, so a heap that is not growing does not collect however long it
/// runs — the counterpart to the test above, and what keeps "armed by default" from meaning
/// "collects constantly".
#[test]
fn a_heap_that_is_not_growing_does_not_collect() {
    let heap = ArcMagrGC::new();
    heap.set_nursery_bytes_for_test(64 * 1024);
    let mut pins = Vec::new();
    for _ in 0..200 {
        pins.push(heap.pin_root(heap.alloc_array(vec![crate::metadata::Value::I64(0); 16])));
    }
    let after_growth = cycles(&heap);
    // Nothing more is allocated; the policy must stay quiet.
    for _ in 0..1000 {
        let _ = heap.used_bytes();
    }
    assert_eq!(cycles(&heap), after_growth, "a static heap must not re-collect");
}

#[test]
fn a_budget_arms_automatic_collection() {
    let heap = ArcMagrGC::new();
    heap.set_nursery_bytes_for_test(16 * 1024);
    heap.set_max_heap_bytes(Some(64 * 1024));
    for _ in 0..4000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    assert!(cycles(&heap) > 0,
        "a heap budget must arm auto-collect (got {} cycles)", cycles(&heap));
}

#[test]
fn an_over_budget_live_set_does_not_re_collect_forever() {
    // The pathology the futility backoff was added for: a live set that genuinely exceeds the
    // budget makes every collection reclaim ~nothing while the heap keeps growing, so a gate
    // that only asks for growth re-arms forever. Measured on
    // `src/tests/perf/scenarios/09_alloc_ctorless` with a 64MB budget: a 0.29s run had not
    // finished after 9 minutes, doing a 0-byte 75ms mark-sweep every ~6MB.
    //
    // **arm-gc-by-default (2026-09-09)** attacks it from the other side as well: the gate is
    // now `live × ALLOWANCE_HEAP_RATIO`, so it *grows with the live set* and the collection
    // count is logarithmic in heap growth rather than linear. The backoff stays as a belt for
    // the case a soft cap squeezes the allowance down to its floor — which is what this test
    // sets up.
    //
    // Each retained allocation is made with a wide-open nursery: the inline fallback collects
    // at the tail of the allocation that trips the gate, so a tight gate can tombstone the
    // fresh value before the caller can root it (production defers to a safepoint, where it is
    // already a frame-reg root).
    let heap = ArcMagrGC::new();
    let budget = 32 * 1024;
    heap.set_max_heap_bytes(Some(budget));
    let mut pins = Vec::new();
    for _ in 0..128 {
        heap.set_nursery_bytes_for_test(1 << 30);
        let v = heap.alloc_array(vec![crate::metadata::Value::I64(0); 128]);
        pins.push(heap.pin_root(v));
        heap.set_nursery_bytes_for_test(4 * 1024);
        // One tiny throwaway per round: enough that collections have *something* to reclaim,
        // far less than one gate's worth — which is exactly what "futile" means here. The
        // retained arrays supply the growth.
        let _ = heap.alloc_array(vec![crate::metadata::Value::I64(0); 4]);
    }
    let n = cycles(&heap);
    assert!(
        n <= 20,
        "an over-budget live set must stop re-collecting; got {n} cycles over 128 rounds"
    );
    drop(pins);
}

#[test]
fn a_reclaiming_collector_keeps_collecting_as_the_heap_refills() {
    // The growth gate (rule 2) measures from the *end* of the last collection.
    // Measured from the last trip instead — the pre-collect high-water, which
    // rule 1 pins at `near_limit_ratio × budget` — re-tripping demanded
    // `(0.90 + 0.10) × budget` and up, i.e. the heap had to climb past the
    // whole budget before a second cycle. A collector doing its job keeps it
    // below that, so the budget silently stopped being enforced after cycle 1:
    // `z42c.semantics --release --no-incremental` under a 256MB budget
    // collected exactly once in a run that allocated 450MB, ending 17MB over.
    //
    // Nothing here is rooted, so every cycle reclaims essentially all of it and
    // the heap refills from near zero — the case the old baseline could not see.
    let heap = ArcMagrGC::new();
    let budget = 64 * 1024;
    heap.set_max_heap_bytes(Some(budget));
    for _ in 0..20_000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    let used = heap.stats().used_bytes;
    assert!(cycles(&heap) > 0, "auto-collect should fire");
    assert!(used <= budget,
        "a reclaimable heap must be held at its budget; ended at {used} bytes over a \
         {budget}-byte budget (the pre-trip baseline ends at 163392 — 2.5x over, having \
         ratcheted its own trip point up by one growth gate per cycle)");
}

// ── add-bounded-nursery (2026-09-08) ─────────────────────────────────────────

/// The nursery gate is a *separate* trip condition from the budget gate: under
/// `GenerationalMarkSweep` a minor fires once `Z42_GC_NURSERY_BYTES` has been allocated,
/// without waiting for `used` to climb to `gc_near_limit_ratio × budget`. That is what bounds
/// minor work — and therefore minor pause — by the nursery rather than by the whole heap.
#[test]
fn generational_trips_a_minor_on_the_nursery_gate_below_the_near_limit() {
    use crate::gc::GcMode;
    // A budget large enough that `used` never gets near `0.90 × budget`: under the old
    // single gate this workload collected zero times.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    const BUDGET: u64 = 8 * 1024 * 1024;
    heap.set_nursery_bytes_for_test(64 * 1024);
    heap.set_max_heap_bytes(Some(BUDGET));
    for _ in 0..20_000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    let used = heap.stats().used_bytes;
    assert!(
        used < BUDGET * 9 / 10,
        "test setup: `used` must stay below the near-limit gate (got {used})"
    );
    assert!(
        cycles(&heap) > 0,
        "the nursery gate must trip a minor before the budget gate would"
    );
}

/// **flip-gc-default-to-generational (2026-09-10)**: a soft cap has to be enforced under the
/// generational collector too.
///
/// The generational gate is the **nursery** — an absolute 32 MB by default, deliberately
/// independent of any budget — so with a budget far below one nursery the policy was never
/// consulted: `next_collect_at` sat at `live + 32 MB` and the heap sailed past its cap without
/// collecting once. (`decide_trip` would have called for a major on `near_cap`; it just never
/// got asked.) The fix is `minor_gate` = `min(nursery, allowance)`, and the allowance is what a
/// soft cap squeezes.
///
/// This was invisible while STW was the default — its gate is the allowance, already squeezed.
#[test]
fn generational_enforces_a_soft_cap_far_below_one_nursery() {
    use crate::gc::GcMode;
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    // Deliberately does NOT touch the nursery: a 64 KB budget against the 32 MB default is
    // exactly the shape that went unenforced.
    let budget = 64 * 1024;
    heap.set_max_heap_bytes(Some(budget));
    for _ in 0..20_000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    let used = heap.stats().used_bytes;
    assert!(cycles(&heap) > 0, "a budget below one nursery must still trip a collection");
    assert!(used <= budget,
        "a reclaimable generational heap must be held at its budget; ended at {used} bytes \
         over a {budget}-byte budget (before the fix it ran to ~32MB before collecting once)");
}

/// The two modes size their growth gate differently, and that is the whole point of the
/// nursery: a minor only has to look at the young set, so it may run after one nursery's
/// worth of allocation, while a full collection has to earn its cost and waits for a whole
/// allowance (`ALLOWANCE_NURSERY_RATIO` = 4 nurseries at the floor).
#[test]
fn generational_collects_more_often_than_stw_on_the_same_workload() {
    fn cycles_for(generational: bool) -> u64 {
        let heap = ArcMagrGC::new();
        // flip-gc-default-to-generational: both arms select their mode explicitly — leaving
        // one to the default made this compare generational against generational.
        heap.set_mode(if generational {
            crate::gc::GcMode::GenerationalMarkSweep
        } else {
            crate::gc::GcMode::StwMarkSweep
        });
        heap.set_nursery_bytes_for_test(64 * 1024);
        for _ in 0..20_000 {
            heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
        }
        cycles(&heap)
    }
    let (gen, stw) = (cycles_for(true), cycles_for(false));
    assert!(stw > 0, "STW is armed too — it just waits for a full allowance (got {stw})");
    assert!(
        gen > stw,
        "one nursery per minor must trip more often than one allowance per full collection \
         (generational {gen} vs stw {stw})"
    );
}
