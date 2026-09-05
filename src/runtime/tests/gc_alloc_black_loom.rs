//! loom model of the **new-object sweep hazard** in `GcMode::ConcurrentMarkSweep`
//! — i.e. what marking-period *allocate-black* is for.
//!
//! Tracked by docs/spec/changes/investigate-concurrent-gc-stale-mark-race
//! (phase 3, task 3.1c). Sibling file `gc_registration_race_loom.rs` holds the
//! other two models (stale mark on an *existing* object; collector-arbitration
//! deadlock). Deliberately self-contained: this model needs the full concurrent
//! cycle (snapshot → yield → handshake → sweep) that those two do not model, and
//! a model checker's value depends on each model being readable on its own.
//!
//! ## The hazard, as the production code stands today
//!
//! `finish_alloc` / `alloc_array_obj` (`gc/arc_heap/alloc.rs`) publish a fresh
//! region entry and **never touch its mark bit** — every object is born white
//! (`marked = 0`). Nothing else covers a *newly allocated* object either:
//!
//! - the write barrier (`write_barrier_field`, gc/arc_heap/generational.rs:288)
//!   shades the *stored* ref gray, so it only covers objects written into a heap
//!   field — not one held solely in a frame reg;
//! - `snapshot_roots_into_mark_queue` (gc/arc_heap/roots.rs:21) does walk frame
//!   regs (via the `external_root_scanner` installed in vm_context/construct.rs),
//!   but the concurrent path calls it **once**, in Phase 1, and never re-scans
//!   roots before the Phase 6 sweep (`collect_cycles_with_context`,
//!   gc/arc_heap/control.rs:183-208). The STW path's `mark_phase` re-scans roots
//!   at collect time, which is why StwMarkSweep — the production default — is
//!   unaffected.
//!
//! ⇒ an object allocated **during the concurrent window** and reachable only
//! from a frame reg is never shaded by anything, and Phase 6 tombstones it while
//! it is still reachable. That is strictly worse than a stale mark bit: the
//! mutator keeps a live handle to a tombstoned entry.
//!
//! Note this is *unconditional* — it does not depend on the registration-window
//! race or on any candidate fix. design.md originally framed allocate-black as a
//! prerequisite for the "naive candidate B" barrier change; the model here shows
//! the hazard exists on its own.
//!
//! ## What the model discriminates
//!
//! `AllocBlack` is the policy under test, and the three tests below pin down
//! exactly where the boundary is:
//!
//! | policy | result |
//! |---|---|
//! | `Never` (today) | reachable new object swept |
//! | `ConcurrentOnly` (phase == ConcurrentMarking) | **still** swept — the mutator can allocate after the handshake flipped the phase to `Marking` but before it reaches its next safepoint |
//! | `ConcurrentAndMarking` | green under exhaustive search |
//!
//! The middle row is the point of building this: `ConcurrentOnly` is the obvious
//! reading of "allocate black during the concurrent mark", and it is wrong.
//! design.md's `phase ∈ {ConcurrentMarking, Marking}` is now *proved* necessary
//! rather than asserted.
//!
//! Allocating while the phase is `Idle` / `Requested` is safe **without** any
//! allocate-black, because the Phase 1 root snapshot has not run yet and will
//! shade the object as a frame-reg root — the model reproduces that too (it is
//! why `ConcurrentAndMarking` is green rather than "green by accident").
//!
//! ## Out of scope here
//!
//! Collector arbitration (`collector_active`) — one collector thread only; see
//! model B in the sibling file. Objects shaded by the write barrier — that path
//! already works and modelling it would only add states.
//!
//! Run: `RUSTFLAGS="--cfg loom" cargo test --manifest-path src/runtime/Cargo.toml \
//!       --test gc_alloc_black_loom --release`

#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use loom::sync::{Condvar, Mutex};
use loom::thread;

// gc/safepoint.rs `GcPhase`.
const IDLE: usize = 0;
const REQUESTED: usize = 1;
const MARKING: usize = 2;
const CONCURRENT_MARKING: usize = 3;

/// Where `alloc_object` shades a newborn object black (`marked = 1`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AllocBlack {
    /// Production today: never. The entry is born white.
    Never,
    /// The obvious-but-insufficient reading: only while the phase reads
    /// `ConcurrentMarking`.
    ConcurrentOnly,
    /// design.md's proposal: `phase ∈ {ConcurrentMarking, Marking}`.
    ConcurrentAndMarking,
}

struct Gc {
    phase: Mutex<usize>,
    cv: Condvar,
    parked: AtomicUsize,
    /// models `vm_contexts.len()`; 2 = collector + the one mutator.
    num_ctx: AtomicUsize,
    /// The mutator's object exists as a region entry (`alive = 1`).
    obj_alive: AtomicBool,
    /// Its mark bit.
    obj_marked: AtomicBool,
    /// Whether a live frame reg still points at it — i.e. whether it is a GC
    /// root. Cleared when the mutator's frame pops.
    obj_reachable: AtomicBool,
    /// Sweep tombstoned it.
    obj_swept: AtomicBool,
}

impl Gc {
    fn new() -> Self {
        Gc {
            phase: Mutex::new(IDLE),
            cv: Condvar::new(),
            parked: AtomicUsize::new(0),
            num_ctx: AtomicUsize::new(2),
            obj_alive: AtomicBool::new(false),
            obj_marked: AtomicBool::new(false),
            obj_reachable: AtomicBool::new(false),
            obj_swept: AtomicBool::new(false),
        }
    }
}

/// loom 0.7.2 aborts the process if a `loom::sync::Arc` is dropped while
/// unwinding out of a *detected deadlock*; a leaked `&'static` has no
/// destructor and sidesteps that entirely. Same reason as the sibling file —
/// keeping it uniform means a deadlock introduced by a future edit surfaces as
/// a clean `#[should_panic]`-able panic instead of an aborted process.
fn leak_gc() -> &'static Gc {
    Box::leak(Box::new(Gc::new()))
}

// ── mutator side (gc/safepoint.rs) ────────────────────────────────────────

/// `park_until_idle`: park until the world is released. Note the real predicate
/// releases on `Idle` **or** `ConcurrentMarking` — the concurrent path's whole
/// point is that mutators resume for the mark.
fn park_until_released(gc: &Gc) {
    gc.parked.fetch_add(1, Ordering::AcqRel);
    let mut ph = gc.phase.lock().unwrap();
    gc.cv.notify_all();
    while *ph == REQUESTED || *ph == MARKING {
        ph = gc.cv.wait(ph).unwrap();
    }
    gc.parked.fetch_sub(1, Ordering::AcqRel);
}

fn check_safepoint(gc: &Gc) {
    let ph = *gc.phase.lock().unwrap();
    if ph == REQUESTED || ph == MARKING {
        park_until_released(gc);
    }
}

/// `finish_alloc`: publish a fresh region entry. Today the mark bit is simply
/// never touched (`AllocBlack::Never`); the other policies are the candidate
/// fix. The phase read and the publish are separate steps, exactly as they would
/// be in the real allocator — that TOCTOU is what `ConcurrentOnly` trips on.
fn alloc_object(gc: &Gc, policy: AllocBlack) {
    let ph = *gc.phase.lock().unwrap();
    let black = match policy {
        AllocBlack::Never => false,
        AllocBlack::ConcurrentOnly => ph == CONCURRENT_MARKING,
        AllocBlack::ConcurrentAndMarking => ph == CONCURRENT_MARKING || ph == MARKING,
    };
    gc.obj_marked.store(black, Ordering::Release);
    // The object goes straight into a frame reg — a GC root, but NOT one the
    // Phase 1 snapshot could have seen, and no write barrier fires because it is
    // never stored into a heap field.
    gc.obj_reachable.store(true, Ordering::Release);
    gc.obj_alive.store(true, Ordering::Release);
}

/// The mutator: one safepoint, one allocation, one more safepoint, then the
/// frame pops and the `VmContext` deregisters (`VmContext::drop` removes itself
/// from `vm_contexts`, which is why the collector re-reads `need` each wakeup).
fn mutator(gc: &Gc, policy: AllocBlack) {
    check_safepoint(gc);
    alloc_object(gc, policy);
    check_safepoint(gc);

    // Frame pops: the object stops being a root.
    gc.obj_reachable.store(false, Ordering::Release);
    // VmContext::drop — deregister, then wake a collector that may be waiting on
    // a `need` that just went down.
    gc.num_ctx.fetch_sub(1, Ordering::AcqRel);
    let _ph = gc.phase.lock().unwrap();
    gc.cv.notify_all();
}

// ── collector side ────────────────────────────────────────────────────────

/// `collect_cycles_with_context` under `GcMode::ConcurrentMarkSweep`
/// (gc/arc_heap/control.rs:170-240).
fn collector_cycle(gc: &Gc) {
    // request_gc_pause: Idle → Requested, wait for the handshake, → Marking.
    *gc.phase.lock().unwrap() = REQUESTED;
    {
        let mut ph = gc.phase.lock().unwrap();
        loop {
            let need = gc.num_ctx.load(Ordering::Acquire).saturating_sub(1);
            if gc.parked.load(Ordering::Acquire) >= need {
                break;
            }
            ph = gc.cv.wait(ph).unwrap();
        }
        *ph = MARKING;
    }

    // Phase 1: STW root snapshot. The external root scanner walks every live
    // VmContext's frame regs, so an object already sitting in one IS shaded
    // here. This runs exactly once per cycle — there is no re-scan later.
    if gc.obj_reachable.load(Ordering::Acquire) {
        gc.obj_marked.store(true, Ordering::Release);
    }

    // Phase 2: yield_to_concurrent_marking — mutators resume.
    {
        let mut ph = gc.phase.lock().unwrap();
        *ph = CONCURRENT_MARKING;
        gc.cv.notify_all();
    }

    // Phase 3: background drain (nothing to trace in this model).

    // Phase 4: request_handshake_pause — back to Marking, re-park everyone.
    {
        let mut ph = gc.phase.lock().unwrap();
        *ph = MARKING;
        gc.cv.notify_all();
        loop {
            let need = gc.num_ctx.load(Ordering::Acquire).saturating_sub(1);
            if gc.parked.load(Ordering::Acquire) >= need {
                break;
            }
            ph = gc.cv.wait(ph).unwrap();
        }
    }

    // Phase 5: residual drain (nothing).

    // Phase 6: STW sweep — alive entries keep their mark cleared, unmarked ones
    // are tombstoned.
    if gc.obj_alive.load(Ordering::Acquire) {
        if gc.obj_marked.load(Ordering::Acquire) {
            gc.obj_marked.store(false, Ordering::Release); // survivor → white for next cycle
        } else {
            gc.obj_alive.store(false, Ordering::Release);
            gc.obj_swept.store(true, Ordering::Release);
        }
    }

    // debug_validate_invariants, still under STW. Sweeping an object that no
    // frame reg points at any more is correct (it is garbage); sweeping one that
    // is still a root is the bug.
    assert!(
        !(gc.obj_swept.load(Ordering::Acquire) && gc.obj_reachable.load(Ordering::Acquire)),
        "reachable object allocated during the concurrent cycle was swept \
         (needs marking-period allocate-black)"
    );

    // GcPauseGuard::drop — release the world.
    let mut ph = gc.phase.lock().unwrap();
    *ph = IDLE;
    gc.cv.notify_all();
}

fn run_model(policy: AllocBlack) {
    loom::model::Builder::new().check(move || {
        let gc = leak_gc();
        let m = thread::spawn(move || mutator(gc, policy));
        collector_cycle(gc);
        m.join().unwrap();
    });
}

/// Production today: the object is born white and nothing ever shades it, so the
/// Phase 6 sweep tombstones it while a frame reg still points at it.
#[test]
#[should_panic(expected = "was swept")]
fn new_object_is_swept_without_allocate_black() {
    run_model(AllocBlack::Never);
}

/// Shading black only while the phase reads `ConcurrentMarking` is NOT enough:
/// `request_handshake_pause` flips the phase to `Marking` and then *waits* for
/// mutators to park, so a mutator can still allocate — white — in that window,
/// before it reaches its next safepoint. This is the reason the fix must cover
/// `Marking` as well; keep this test as the guard against narrowing it back.
#[test]
#[should_panic(expected = "was swept")]
fn allocate_black_on_concurrent_marking_alone_is_insufficient() {
    run_model(AllocBlack::ConcurrentOnly);
}

/// `phase ∈ {ConcurrentMarking, Marking}` — design.md's proposal. Green under
/// exhaustive search. Allocations at `Idle` / `Requested` stay white and are
/// still safe: the Phase 1 snapshot has not run yet and shades them as frame-reg
/// roots.
#[test]
fn allocate_black_on_concurrent_and_marking_is_sufficient() {
    run_model(AllocBlack::ConcurrentAndMarking);
}
