//! loom models of the ConcurrentMarkSweep registration/handshake hazards.
//!
//! Tracked by docs/spec/changes/investigate-concurrent-gc-stale-mark-race
//! (phase 3). Neither hazard reproduces on local hardware — the design
//! amplified `concurrent_gc_mode_stress_no_race_no_leak` to 8×2000×4000 and it
//! still passed on Apple Silicon; it only fires on some CI runners (windows-x86,
//! and — 2026-07-08 — macos-arm64). loom explores thread interleavings
//! deterministically, so both hazards reproduce here, locally, every run.
//!
//! There are TWO models, because the fix has to survive both at once:
//!
//! | model | scenario | search | what a green run means |
//! |---|---|---|---|
//! | A — stale mark | one collector, one late-registering mutator | preemption-bounded (3) | the fix closes the registration→sweep window |
//! | B — arbitration | an active collector releases while a worker is parked | **exhaustive** (34 interleavings) | the fix does NOT re-introduce the 2026-06-01 deadlock |
//!
//! Model B exists because a fix that only satisfies A is a trap: the
//! 2026-06-01 "park at registration" attempt made A green and **deadlocked**
//! `safepoint_tests::second_collector_falls_back_to_mutator_park_returns_none`.
//!
//! ## What is modelled (faithful to gc/safepoint.rs + arc_heap sweep/validate)
//!
//! - `phase`   : Idle → Requested → Marking → Idle  (gc/safepoint.rs `GcPhase`)
//! - `num_ctx` : models `vm_contexts.len()`; the collector's handshake target is
//!               `need = num_ctx - 1` (`request_gc_pause`, safepoint.rs:377-384).
//! - `parked`  : `parked_count` — the collector waits until `parked >= need`,
//!               RE-READING `num_ctx` each wakeup (the existing defense against
//!               a freshly-registered context).
//! - `collector_active`: the add-multi-collector-arbitration (2026-05-21) CAS
//!               claim (safepoint.rs:353). A thread losing the CAS parks *as a
//!               mutator* and skips its own collect (real code: returns `None`).
//! - `obj_mark`: one *alive* object's mark bit. The write barrier shades it gray
//!               (marked=1); sweep clears survivor marks back to white; the
//!               post-sweep invariant `debug_validate_invariants` asserts no alive
//!               object is still marked (arc_heap.rs "stale mark bit … after sweep").
//!
//! ## Model A — the stale-mark race
//!
//! A mutator that registers LATE — after the collector's `need` snapshot already
//! read `num_ctx` and broke out to `Marking` — runs a write barrier before its
//! first safepoint, marking the alive object AFTER sweep cleared it → the mark is
//! stale at validate. The collector's per-wakeup re-read only helps while it is
//! still *waiting*; once `need` was momentarily satisfied (e.g. `need == 0`) it
//! stops re-reading, and a later registration escapes the handshake entirely.
//!
//! ## Model B — the collector-arbitration deadlock
//!
//! Mirrors `second_collector_falls_back_to_mutator_park_returns_none` exactly:
//! the test-main thread poses as an active collector (`collector_active = true`,
//! phase `Marking`), waits for the worker to park, then releases and `join()`s.
//! **After that join begins, main never parks again — but its `VmContext` stays
//! registered in `vm_contexts`, so it still counts toward `need`.** That
//! asymmetry is the whole deadlock:
//!
//! - baseline: the worker parks *inside* `request_gc_pause`, after its CAS
//!   already lost → it can never become collector → returns None → clean join.
//! - "park at registration" fix: the worker parks *before* the CAS. By the time
//!   it wakes, main has released the claim → the worker **wins** the CAS,
//!   becomes collector, and waits for `need = 1` parkers that will never come,
//!   while main waits in `join()`. loom reports the deadlock.
//!
//! So registration-window closure must not move a context's park to *before*
//! the collector CAS. Any candidate fix has to keep `arbitration_*` green.
//!
//! ## Scope
//!
//! The third hazard — the *new-object* sweep hazard that marking-period
//! allocate-black fixes, separate from this stale-mark-on-an-*existing*-object
//! race — lives in the sibling file `gc_alloc_black_loom.rs` (model C). A fix
//! that closes the registration window but keeps `alloc_object` birthing
//! objects white is still unsound, so a candidate fix has to keep all three
//! files green.
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
    /// add-multi-collector-arbitration (2026-05-21): the exclusive collector claim.
    collector_active: AtomicBool,
}

impl Gc {
    fn new() -> Self {
        Gc {
            phase: Mutex::new(IDLE),
            cv: Condvar::new(),
            parked: AtomicUsize::new(0),
            num_ctx: AtomicUsize::new(1),
            obj_mark: AtomicBool::new(false),
            collector_active: AtomicBool::new(false),
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

/// Collector-side entry, modelling `gc/safepoint.rs::request_gc_pause`.
///
/// Returns `true` when this thread claimed the collector role (real code:
/// `Some(GcPauseGuard)`), `false` when another collector already held it — in
/// which case we park-as-mutator first, exactly like the real fallback (real
/// code: `None`, caller skips its collect).
fn request_gc_pause(gc: &Gc) -> bool {
    if gc
        .collector_active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        park_until_idle(gc);
        return false;
    }

    *gc.phase.lock().unwrap() = REQUESTED;

    // Wait for everyone-but-self to park, re-reading num_ctx each wakeup.
    let mut ph = gc.phase.lock().unwrap();
    loop {
        let need = gc.num_ctx.load(Ordering::Acquire).saturating_sub(1);
        if gc.parked.load(Ordering::Acquire) >= need {
            break;
        }
        ph = gc.cv.wait(ph).unwrap();
    }
    *ph = MARKING;
    true
}

/// `GcPauseGuard::drop`: open the world, notify, then release the claim.
fn release_pause(gc: &Gc) {
    *gc.phase.lock().unwrap() = IDLE;
    gc.cv.notify_all();
    gc.collector_active.store(false, Ordering::Release);
}

// ── Model A: registration → sweep stale mark ──────────────────────────────

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

/// The collector: the arbitration CAS + handshake, then sweep + the post-sweep
/// stale-mark invariant.
fn collector(gc: &Gc) {
    assert!(
        request_gc_pause(gc),
        "single-collector model: this CAS is uncontended and must win"
    );
    // Sweep: clear the survivor's mark back to white for the next cycle.
    gc.obj_mark.store(false, Ordering::Release);
    // debug_validate_invariants: no ALIVE object may still be marked after sweep.
    assert!(
        !gc.obj_mark.load(Ordering::Acquire),
        "stale mark bit on alive object after sweep (registration→sweep race)"
    );
    release_pause(gc);
}

/// Preemption-bounded search, used by **model A only**: three free-running
/// threads plus Condvar wait/notify make the *exhaustive* state space blow up
/// (esp. the no-race fixed path, which never short-circuits). A preemption bound
/// of 3 keeps it tractable while still exercising the
/// register-vs-handshake-vs-sweep interleavings — the race reproduces at bound 3
/// (and is the FIRST failure found), so the bound is sufficient for this window.
///
/// Model B needs no bound: its protocol is self-throttling (test-main cannot
/// release until the worker has parked), so the *exhaustive* search is only 34
/// interleavings and runs instantly. See `run_arbitration_model`.
fn bounded_model() -> loom::model::Builder {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder
}

/// Model state as a leaked `&'static Gc` instead of a `loom::sync::Arc`.
///
/// loom 0.7.2 **aborts the process** when a `loom::sync::Arc` is dropped while
/// unwinding out of a detected deadlock: the drop calls `rt::arc::Arc::branch`,
/// which unwraps the already-torn-down execution → panic-in-a-destructor →
/// "thread caused non-unwinding panic. aborting." That would make `#[should_panic]`
/// unusable for model B (and leaves a stuck `UE` process behind). A plain
/// `&'static Gc` has no destructor, so the deadlock panic unwinds cleanly and the
/// harness observes it. Cost is one small struct leaked per explored execution.
fn leak_gc() -> &'static Gc {
    Box::leak(Box::new(Gc::new()))
}

fn run_model(registration_close: bool) {
    bounded_model().check(move || {
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
/// no-race search minutes-long. Note that a green here is NOT on its own evidence
/// that the fix is correct — model B below shows this very fix deadlocks under
/// collector arbitration. Run explicitly:
///   RUSTFLAGS="--cfg loom" cargo test --test gc_registration_race_loom --release \
///     -- --ignored registration_close_eliminates_race
#[test]
#[ignore = "slow under Condvar even preemption-bounded; and green here is insufficient — see model B"]
fn registration_close_eliminates_race() {
    run_model(true);
}

// ── Model B: collector arbitration → the 2026-06-01 deadlock ──────────────

/// The worker of `second_collector_falls_back_to_mutator_park_returns_none`:
/// register a fresh `VmContext`, then immediately try to collect.
///
/// With `registration_close` the park moves to BEFORE the arbitration CAS —
/// which is precisely what turns a clean `None` fallback into a deadlock.
/// Returns whether this worker ended up claiming the collector role.
fn worker_registers_then_collects(gc: &Gc, registration_close: bool) -> bool {
    gc.num_ctx.fetch_add(1, Ordering::AcqRel); // VmContext::new_with_core

    if registration_close {
        let in_flight = { *gc.phase.lock().unwrap() != IDLE };
        if in_flight {
            park_until_idle(gc);
        }
    }

    let won = request_gc_pause(gc);
    if won {
        release_pause(gc);
    }
    won
}

/// Model B is **exhaustive** — no preemption bound. Test-main can't release the
/// world until the worker has parked, which prunes the space to 34 interleavings
/// (measured); an unbounded search costs ~0.1s, so a green here really does mean
/// "no interleaving deadlocks", not "none within 3 preemptions".
fn run_arbitration_model(registration_close: bool) {
    loom::model::Builder::new().check(move || {
        let gc = leak_gc();

        // Test-main poses as an already-active collector holding the world in
        // Marking (the unit test's two explicit stores), then spawns the worker.
        gc.collector_active.store(true, Ordering::Release);
        *gc.phase.lock().unwrap() = MARKING;

        let w = thread::spawn(move || worker_registers_then_collects(gc, registration_close));

        // The unit test spins on `parked_count >= 1`; expressed here on the same
        // Condvar the worker notifies under the phase lock.
        {
            let mut ph = gc.phase.lock().unwrap();
            while gc.parked.load(Ordering::Acquire) < 1 {
                ph = gc.cv.wait(ph).unwrap();
            }
        }

        // Release, in the unit test's order: drop the claim first, then open the
        // world and notify.
        gc.collector_active.store(false, Ordering::Release);
        *gc.phase.lock().unwrap() = IDLE;
        gc.cv.notify_all();

        // join(): from here test-main NEVER parks again, yet its VmContext stays
        // registered in vm_contexts and still counts toward any later collector's
        // `need`. That asymmetry is what the deadlock walks into.
        let won = w.join().unwrap();
        assert!(!won, "the losing collector must return None, not claim the role");
    });
}

/// Baseline control: the worker parks only AFTER its CAS has already lost, so it
/// can never claim the role behind main's back. No interleaving deadlocks — this
/// is what keeps model B honest (a model that deadlocks either way proves
/// nothing about the fix).
#[test]
fn arbitration_baseline_has_no_deadlock() {
    run_arbitration_model(false);
}

/// The 2026-06-01 regression, now deterministic: parking at registration moves
/// the worker's park to before the arbitration CAS, so it wakes into a released
/// claim, wins it, and then waits forever for a context whose thread is blocked
/// in `join()`. This test is green when loom reports that deadlock.
///
/// Keep this failing-on-purpose test as the gate for any registration-window
/// fix: a candidate fix must make `registration_close_eliminates_race` green
/// while *also* NOT deadlocking here.
#[test]
#[should_panic(expected = "deadlock")]
fn registration_close_reintroduces_2026_06_01_deadlock() {
    run_arbitration_model(true);
}
