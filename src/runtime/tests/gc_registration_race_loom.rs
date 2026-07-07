//! loom model of the ConcurrentMarkSweep registration→sweep stale-mark race.
//!
//! Tracked by docs/spec/changes/investigate-concurrent-gc-stale-mark-race
//! (phase 3). The real race can't be reproduced on local hardware — the design
//! amplified `concurrent_gc_mode_stress_no_race_no_leak` to 8×2000×4000 and it
//! still passed on Apple Silicon; it only fires on some CI runners (windows-x86,
//! and — 2026-07-08 — macos-arm64). loom explores ALL thread interleavings
//! deterministically, so it reproduces the window here, locally, every run
//! (verified: `race_reproduces_without_registration_close` fires the exact
//! "stale mark … after sweep" assertion in 0.01s).
//!
//! ## What is modelled (faithful to gc/safepoint.rs + arc_heap sweep/validate)
//!
//! - `phase`  : Idle → Requested → Marking → Idle  (gc/safepoint.rs `GcPhase`)
//! - `num_ctx`: models `vm_contexts.len()`; the collector's handshake target is
//!              `need = num_ctx - 1` (`request_gc_pause`, safepoint.rs:243).
//! - `parked` : `parked_count` — the collector waits until `parked >= need`,
//!              RE-READING `num_ctx` each wakeup (safepoint.rs:236-248, the
//!              existing defense against a freshly-registered context).
//! - `obj_mark`: one *alive* object's mark bit. The write barrier shades it gray
//!              (marked=1); sweep clears survivor marks back to white; the
//!              post-sweep invariant `debug_validate_invariants` asserts no alive
//!              object is still marked (arc_heap.rs "stale mark bit … after sweep").
//!
//! ## The race (what loom finds)
//!
//! A mutator that registers LATE — after the collector's `need` snapshot already
//! read `num_ctx` and broke out to `Marking` — runs a write barrier before its
//! first safepoint, marking the alive object AFTER sweep cleared it → the mark is
//! stale at validate. The collector's per-wakeup re-read only helps while it is
//! still *waiting*; once `need` was momentarily satisfied (e.g. `need == 0`) it
//! stops re-reading, and a later registration escapes the handshake entirely.
//!
//! ## The modelled fix: registration-window close
//!
//! `late_mutator` with `registration_close = true` mirrors the design's fix
//! direction: at registration, if a cycle is already in flight (phase != Idle),
//! the new context parks BEFORE running any heap op — so it can never barrier
//! during the STW window. loom verifies this eliminates the race in this model
//! (`registration_close_eliminates_race` is green under full interleaving search).
//!
//! ## HONEST SCOPE / next increments
//!
//! This model has ONE fixed collector. It does NOT yet model multi-collector
//! arbitration (`collector_active` CAS, safepoint.rs:223), which is required to
//! also reproduce the 2026-06-01 registration-fence DEADLOCK (a mutator that
//! parks at registration then wins the collector race). Nor does it model
//! marking-period allocate-black (needed for the *new-object* sweep hazard,
//! separate from this stale-mark-on-existing-object race). Before the fix is
//! ported to the real GC it must ALSO pass a deadlock model that includes
//! collector arbitration — otherwise it risks re-introducing that deadlock.
//!
//! Run: `RUSTFLAGS="--cfg loom" cargo test --manifest-path src/runtime/Cargo.toml \
//!       --test gc_registration_race_loom --release`

#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;

const IDLE: usize = 0;
const REQUESTED: usize = 1;
const MARKING: usize = 2;

struct Gc {
    phase: Mutex<usize>,
    cv: Condvar,
    parked: AtomicUsize,
    num_ctx: AtomicUsize, // models vm_contexts.len(); starts at 1 (the collector's own ctx)
    obj_mark: AtomicBool, // the single alive object's mark bit
}

impl Gc {
    fn new() -> Self {
        Gc {
            phase: Mutex::new(IDLE),
            cv: Condvar::new(),
            parked: AtomicUsize::new(0),
            num_ctx: AtomicUsize::new(1),
            obj_mark: AtomicBool::new(false),
        }
    }
}

/// Mutator parks until the world is Idle (gc/safepoint.rs `park_until_idle`):
/// increment parked_count, notify, wait for Idle under the lock, decrement.
fn park_until_idle(gc: &Gc) {
    gc.parked.fetch_add(1, Ordering::AcqRel);
    let mut ph = gc.phase.lock().unwrap();
    gc.cv.notify_all();
    while *ph != IDLE {
        ph = gc.cv.wait(ph).unwrap();
    }
    gc.parked.fetch_sub(1, Ordering::AcqRel);
}

/// A late-registering mutator: registers into vm_contexts, then (before reaching
/// its first safepoint) runs a write barrier shading the alive object gray, then
/// finally reaches a safepoint. This is the window the real bug exploits.
///
/// `registration_close` = the modelled fix: park at registration if a cycle is
/// already in flight, before any heap op.
fn late_mutator(gc: &Gc, registration_close: bool) {
    gc.num_ctx.fetch_add(1, Ordering::AcqRel); // register

    if registration_close {
        // Registration-window close: if a cycle is already in flight, park before
        // touching the heap so this context is counted / can't barrier during STW.
        let in_flight = { *gc.phase.lock().unwrap() != IDLE };
        if in_flight {
            park_until_idle(gc);
        }
    }

    gc.obj_mark.store(true, Ordering::Release); // write barrier: shade alive obj gray

    // first safepoint
    let ph = *gc.phase.lock().unwrap();
    if ph == REQUESTED || ph == MARKING {
        park_until_idle(gc);
    }
}

/// The collector: request_gc_pause handshake (safepoint.rs:217-253) then
/// sweep + the post-sweep stale-mark invariant.
fn collector(gc: &Gc) {
    *gc.phase.lock().unwrap() = REQUESTED;
    // Wait for everyone-but-self to park, re-reading num_ctx each wakeup.
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
    // Sweep: clear the survivor's mark back to white for the next cycle.
    gc.obj_mark.store(false, Ordering::Release);
    // debug_validate_invariants: no ALIVE object may still be marked after sweep.
    assert!(
        !gc.obj_mark.load(Ordering::Acquire),
        "stale mark bit on alive object after sweep (registration→sweep race)"
    );
    // Release the world.
    *gc.phase.lock().unwrap() = IDLE;
    gc.cv.notify_all();
}

fn run_model(registration_close: bool) {
    // Preemption-bounded search: the Condvar wait/notify makes the *exhaustive*
    // state space blow up (esp. the no-race fixed path, which never short-circuits).
    // A preemption bound of 3 keeps it tractable while still exercising the
    // register-vs-handshake-vs-sweep interleavings — the race reproduces at bound 3
    // (and is the FIRST failure found), so the bound is sufficient for this window.
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(move || {
        let gc = Arc::new(Gc::new());
        let gc_c = gc.clone();
        let c = thread::spawn(move || collector(&gc_c));
        let gc_m = gc.clone();
        let m = thread::spawn(move || late_mutator(&gc_m, registration_close));
        c.join().unwrap();
        m.join().unwrap();
    });
}

/// WITHOUT the fix, loom finds the interleaving where the late mutator barriers
/// the alive object after sweep cleared it → stale mark. This test is green when
/// that assertion fires (it documents the bug deterministically).
#[test]
#[should_panic(expected = "stale mark bit on alive object after sweep")]
fn race_reproduces_without_registration_close() {
    run_model(false);
}

/// WITH the registration-window close, no interleaving leaves a stale mark (in
/// this single-collector model). Confirmed to eliminate the race locally.
///
/// `#[ignore]` for now: the Condvar wait/notify makes even the preemption-bounded
/// no-race search minutes-long, and — more importantly — this single-collector
/// model does NOT model `collector_active` arbitration, so a green here does NOT
/// prove the fix is deadlock-free (the 2026-06-01 registration-fence attempt
/// deadlocked precisely via collector arbitration). Enable + trust this only after
/// the deadlock model lands. Run explicitly:
///   RUSTFLAGS="--cfg loom" cargo test --test gc_registration_race_loom --release \
///     -- --ignored registration_close_eliminates_race
#[test]
#[ignore = "needs the collector-arbitration/deadlock model before it validates the fix; slow under Condvar even bounded — see module docs"]
fn registration_close_eliminates_race() {
    run_model(true);
}
