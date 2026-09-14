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
//! | B′ — straddling registration | a worker's registration straddles a pause while test-main blocks in `join()` | **exhaustive** | a thread blocked in `join()` must be parked — with or without the fix |
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
//! ## The fix that landed — wait out `Marking`, *then* register (fix-context-joins-mid-pause, 2026-09-15)
//!
//! [`Fix::WaitOutMarking`] mirrors `VmContext::new_with_core`: under the phase lock, wait
//! while the phase is `Marking`, then push into `vm_contexts`. The collector's final
//! count-and-flip to `Marking` happens under the same lock, so a registration is either
//! counted before the flip (and the collector waits for it to park) or happens after the
//! pause ends. Model A goes green.
//!
//! It does **not** park, so model B's protocol (test-main waits for the worker to park) no
//! longer fits: a worker waiting out a pause is unregistered and uncounted. Model B′ asks the
//! question that matters instead. The waiting worker still reaches the arbitration CAS only
//! after the pause, so it can win the collector role — and then it waits for every
//! registered context, including test-main blocked in `join()`. Model B′ shows:
//!
//! - that deadlock is **not new**: with no fix at all, a worker registering just after the
//!   release wins the CAS the same way (`unparked_join_deadlocks_even_without_a_fix`).
//!   2026-06-01's attempt did not create the hazard; it made the unit test hit it every time.
//! - it goes away once the blocked thread is parked, as every blocking native call in the
//!   runtime now is (`Thread.Join` #598, the rest #648): `a_parked_joiner_never_deadlocks_*`.
//!
//! So the rule the fix relies on is "a thread blocked outside the VM is parked", not "the
//! worker loses the CAS".
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

/// How a new context joins `vm_contexts` (`VmContext::new_with_core`).
#[derive(Clone, Copy)]
enum Fix {
    /// Before 2026-09-15: push unconditionally.
    None,
    /// The 2026-06-01 attempt: push, then park if a cycle is in flight — a park *before*
    /// the arbitration CAS.
    ParkAtRegistration,
    /// What landed (fix-context-joins-mid-pause): under the phase lock, wait while `Marking`,
    /// then push. Not a park — the waiter is not registered, so no collector counts it.
    WaitOutMarking,
}

/// `VmContext::new_with_core`'s registration step under each [`Fix`].
fn register(gc: &Gc, fix: Fix) {
    match fix {
        Fix::None => {
            gc.num_ctx.fetch_add(1, Ordering::AcqRel);
        }
        Fix::ParkAtRegistration => {
            gc.num_ctx.fetch_add(1, Ordering::AcqRel);
            let in_flight = { *gc.phase.lock().unwrap() != IDLE };
            if in_flight {
                park_until_idle(gc);
            }
        }
        Fix::WaitOutMarking => {
            let mut ph = gc.phase.lock().unwrap();
            while *ph == MARKING {
                ph = gc.cv.wait(ph).unwrap();
            }
            gc.num_ctx.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// A late-registering mutator: registers into vm_contexts, then (before reaching
/// its first safepoint) runs a write barrier shading the alive object gray, then
/// finally reaches a safepoint. This is the window the real bug exploits.
fn late_mutator(gc: &Gc, fix: Fix) {
    register(gc, fix);

    gc.obj_mark.store(true, Ordering::Release); // write barrier: shade alive obj gray

    // first safepoint
    let ph = *gc.phase.lock().unwrap();
    if ph == REQUESTED || ph == MARKING {
        park_until_idle(gc);
    }

    unregister(gc);
}

/// `impl Drop for VmContext`: leave `vm_contexts`, then wake a waiting collector under the
/// phase lock so it re-reads its target.
///
/// Without this a mutator that passes its only safepoint while the world is still `Idle`
/// would end the model still counted, and a collector starting afterwards would wait for
/// it forever — a deadlock of the model, not of the runtime. The no-fix run never got that
/// far (the stale mark is found first); a green run of a fix does.
fn unregister(gc: &Gc) {
    gc.num_ctx.fetch_sub(1, Ordering::AcqRel);
    let _ph = gc.phase.lock().unwrap();
    gc.cv.notify_all();
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

fn run_model(fix: Fix) {
    bounded_model().check(move || {
        let gc = Arc::new(Gc::new());
        let gc_c = gc.clone();
        let c = thread::spawn(move || collector(&gc_c));
        let gc_m = gc.clone();
        let m = thread::spawn(move || late_mutator(&gc_m, fix));
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
    run_model(Fix::None);
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
    run_model(Fix::ParkAtRegistration);
}

/// The fix that landed closes the window: a registration is serialized against the
/// collector's count-and-flip to `Marking`, so it is either waited for or happens after
/// the sweep. Unlike the park-at-registration variant this search is fast — the waiter
/// never enters the parked-count protocol.
#[test]
fn waiting_out_marking_eliminates_race() {
    run_model(Fix::WaitOutMarking);
}

// ── Model B: collector arbitration → the 2026-06-01 deadlock ──────────────

/// The worker of `second_collector_falls_back_to_mutator_park_returns_none`:
/// register a fresh `VmContext`, then immediately try to collect.
///
/// With `Fix::ParkAtRegistration` the park moves to BEFORE the arbitration CAS —
/// which is precisely what turns a clean `None` fallback into a deadlock.
/// Returns whether this worker ended up claiming the collector role.
fn worker_registers_then_collects(gc: &Gc, fix: Fix) -> bool {
    register(gc, fix);

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
fn run_arbitration_model(fix: Fix) {
    loom::model::Builder::new().check(move || {
        let gc = leak_gc();

        // Test-main poses as an already-active collector holding the world in
        // Marking (the unit test's two explicit stores), then spawns the worker.
        gc.collector_active.store(true, Ordering::Release);
        *gc.phase.lock().unwrap() = MARKING;

        let w = thread::spawn(move || worker_registers_then_collects(gc, fix));

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
    run_arbitration_model(Fix::None);
}

/// The 2026-06-01 regression, now deterministic: parking at registration moves
/// the worker's park to before the arbitration CAS, so it wakes into a released
/// claim, wins it, and then waits forever for a context whose thread is blocked
/// in `join()`. This test is green when loom reports that deadlock.
///
/// Keep this failing-on-purpose test: it is why the fix that landed waits out
/// `Marking` *unregistered* instead of parking (model B′ below covers that fix,
/// and why the deadlock this test shows is older than any registration change).
#[test]
#[should_panic(expected = "deadlock")]
fn registration_close_reintroduces_2026_06_01_deadlock() {
    run_arbitration_model(Fix::ParkAtRegistration);
}

// ── Model B′: a registration straddling the pause, test-main blocked in join() ──

/// `NativeParkGuard::enter` (gc/safepoint.rs `native_park_incr`): count as parked and
/// wake a waiting collector, notifying under the phase lock.
fn native_park_enter(gc: &Gc) {
    gc.parked.fetch_add(1, Ordering::AcqRel);
    let _ph = gc.phase.lock().unwrap();
    gc.cv.notify_all();
}

/// `NativeParkGuard::drop` (`native_park_decr`): wait out any pause, then uncount.
fn native_park_exit(gc: &Gc) {
    let mut ph = gc.phase.lock().unwrap();
    while *ph != IDLE {
        ph = gc.cv.wait(ph).unwrap();
    }
    gc.parked.fetch_sub(1, Ordering::AcqRel);
}

/// Test-main holds a (simulated) pause, spawns a worker that registers and tries to
/// collect, releases **without** waiting for the worker to do anything — so the worker's
/// registration may land before, during or after the pause — then joins it, parked or not.
///
/// The worker may legitimately win the collector role here (it registers after the release
/// in some interleavings); what must not happen is a deadlock.
fn run_straddle_model(fix: Fix, main_parks_while_joining: bool) {
    loom::model::Builder::new().check(move || {
        let gc = leak_gc();

        gc.collector_active.store(true, Ordering::Release);
        *gc.phase.lock().unwrap() = MARKING;

        let w = thread::spawn(move || worker_registers_then_collects(gc, fix));

        gc.collector_active.store(false, Ordering::Release);
        *gc.phase.lock().unwrap() = IDLE;
        gc.cv.notify_all();

        if main_parks_while_joining {
            native_park_enter(gc);
        }
        w.join().unwrap();
        if main_parks_while_joining {
            native_park_exit(gc);
        }
    });
}

/// The fix's precondition, exhaustively: once a thread blocked in `join()` is parked, a
/// worker that waited out the pause and then won the collector role finds it parked and
/// finishes.
#[test]
fn a_parked_joiner_never_deadlocks_with_waiting_registration() {
    run_straddle_model(Fix::WaitOutMarking, true);
}

/// Control for the model's discriminating power: the same scenario with test-main
/// blocked in `join()` unparked deadlocks — the worker wins the CAS after the pause and
/// waits for test-main forever.
#[test]
#[should_panic(expected = "deadlock")]
fn an_unparked_joiner_deadlocks_with_waiting_registration() {
    run_straddle_model(Fix::WaitOutMarking, false);
}

/// …and it deadlocks exactly the same way with **no fix at all**: a worker that registers
/// just after the release wins the CAS just the same. The hazard 2026-06-01 ran into is
/// an unparked blocked thread, older than any registration change.
#[test]
#[should_panic(expected = "deadlock")]
fn unparked_join_deadlocks_even_without_a_fix() {
    run_straddle_model(Fix::None, false);
}

/// Parking the joiner is also sufficient for today's baseline — the rule is independent
/// of the registration fix.
#[test]
fn a_parked_joiner_never_deadlocks_without_a_fix() {
    run_straddle_model(Fix::None, true);
}
