//! add-gc-runtime-knobs (2026-09-05): auto-collect gating + futility backoff.
//!
//! These drive `maybe_auto_collect` on a bare `ArcMagrGC` (no `VmCore` wiring),
//! so it takes the inline-collect fallback and we can observe `gc_cycles`.

use crate::gc::{ArcMagrGC, MagrGC};

fn cycles(heap: &ArcMagrGC) -> u64 { heap.stats().gc_cycles }

#[test]
fn no_budget_means_no_automatic_collection() {
    // The historical default: `Z42_GC_MAX_BYTES` unset ⇒ `max_bytes` is None ⇒
    // auto-collect never trips, no matter how much is allocated.
    let heap = ArcMagrGC::new();
    for _ in 0..2000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    assert_eq!(cycles(&heap), 0,
        "with no heap budget the GC must never auto-collect (pre-existing default)");
}

#[test]
fn a_budget_arms_automatic_collection() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(64 * 1024));
    for _ in 0..4000 {
        heap.alloc_array(vec![crate::metadata::Value::I64(0); 16]);
    }
    assert!(cycles(&heap) > 0,
        "a heap budget must arm auto-collect (got {} cycles)", cycles(&heap));
}

#[test]
fn futile_collections_back_off_instead_of_repeating_forever() {
    // Grow a *live* set past the budget so every collection reclaims almost
    // nothing. Without the backoff the growth gate re-arms forever: measured on
    // `bench/scenarios/09_alloc_ctorless` with a 64MB budget, a 0.29s run had
    // not finished after 9 minutes, doing a 0-byte 75ms mark-sweep every ~6MB.
    //
    // Each retained allocation is made with the budget disarmed: the inline
    // fallback collects at the tail of the allocation that trips the gate, so
    // an armed budget can tombstone the fresh value before the caller can root
    // it (production defers to a safepoint, where it is already a frame-reg root).
    let heap = ArcMagrGC::new();
    let budget = 32 * 1024;
    let mut pins = Vec::new();
    for _ in 0..128 {
        heap.set_max_heap_bytes(None);
        let v = heap.alloc_array(vec![crate::metadata::Value::I64(0); 128]);
        pins.push(heap.pin_root(v));
        heap.set_max_heap_bytes(Some(budget));
        // One tiny throwaway per round: enough that collections have *something*
        // to reclaim, far less than one growth-gate's worth — which is exactly
        // what "futile" means here. The retained arrays supply the growth.
        let _ = heap.alloc_array(vec![crate::metadata::Value::I64(0); 4]);
    }
    let n = cycles(&heap);
    assert!(n > 0, "auto-collect should still fire at least once");
    // Bound is calibrated against the same workload with the backoff disabled,
    // which does 57 cycles — so this is a real discriminator, not a rubber stamp.
    assert!(n <= 15,
        "an over-budget live set must stop re-collecting; got {n} cycles \
         (this workload does 57 with the backoff disabled)");
    drop(pins);
}
