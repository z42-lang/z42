//! Unit tests for `gc/safepoint.rs`. Single-VmContext invariants only —
//! multi-thread end-to-end coverage is in
//! `runtime/tests/cross_thread_smoke.rs::gc_collect_with_concurrent_mutators_no_race`.

use super::*;
use crate::vm_context::VmContext;
use std::sync::atomic::Ordering;

#[test]
fn gc_phase_idle_by_default() {
    let ctx = VmContext::new();
    assert_eq!(*ctx.core.gc_phase.lock(), GcPhase::Idle);
    assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 0);
}

#[test]
fn check_safepoint_idle_is_no_op_fast_path() {
    let ctx = VmContext::new();
    // Loop several times to confirm Idle phase short-circuits without
    // touching parked_count or blocking. If the fast path accidentally
    // parked, this test would hang (only one VmContext, so the wait
    // wouldn't be released by anyone).
    for _ in 0..100 {
        check_safepoint(&ctx);
    }
    assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 0);
    assert_eq!(*ctx.core.gc_phase.lock(), GcPhase::Idle);
}

#[test]
fn request_gc_pause_with_only_self_proceeds_immediately() {
    // Only VmContext is the collector; no other mutators to wait for.
    // Should transition Idle → Requested → Marking without blocking.
    let ctx = VmContext::new();
    {
        let _guard = request_gc_pause(&ctx).expect("uncontended CAS should succeed");
        assert_eq!(*ctx.core.gc_phase.lock(), GcPhase::Marking);
    }
    // Guard dropped → released.
    assert_eq!(*ctx.core.gc_phase.lock(), GcPhase::Idle);
}

#[test]
fn pause_guard_drop_notifies_waiters() {
    // Spawn a 2nd VmContext sharing the core, have it park on a Condvar
    // wait, then collector starts pause + finishes. The parked mutator
    // should be released (parked_count returns to 0).
    let collector = VmContext::new();
    let mutator = VmContext::new_with_core(collector.core_arc());

    // Trip the phase manually to Requested to force the mutator into
    // the slow path the next time it checks. (We do it inside a scope so
    // we don't hold the lock when the mutator tries to take it.)
    {
        *collector.core.gc_phase.lock() = GcPhase::Marking;
    }

    // Spawn a thread that calls check_safepoint on the mutator. It
    // should park.
    //
    // We need an owned ref the worker thread can capture — but
    // Pin<Box<VmContext>> is !Unpin, so we move via Arc<VmCore> and
    // construct a fresh VmContext::new_with_core inside the worker.
    let core = collector.core_arc();
    let worker = std::thread::spawn(move || {
        let m = VmContext::new_with_core(core);
        // add-gc-safepoint-counter-throttling (2026-05-21): force the
        // worker's safepoint check into the slow path immediately so
        // the test doesn't need to call check_safepoint 1024 times.
        m.safepoint_skip.store(1, Ordering::Relaxed);
        check_safepoint(&m);
    });

    // Wait until the worker is parked (parked_count == 1). We may have
    // a tiny race window where the worker hasn't yet incremented; loop
    // with a yield. parking_lot Condvar has no spurious wakes against
    // notify_all in this protocol so this poll is just for startup.
    let start = std::time::Instant::now();
    while collector.core.parked_count.load(Ordering::Acquire) < 1 {
        std::thread::yield_now();
        if start.elapsed() > std::time::Duration::from_secs(5) {
            panic!("worker never parked");
        }
    }

    // Release.
    *collector.core.gc_phase.lock() = GcPhase::Idle;
    collector.core.gc_phase_cv.notify_all();
    worker.join().expect("worker panicked");
    assert_eq!(collector.core.parked_count.load(Ordering::Acquire), 0);

    // Keep mutator alive across the asserts so the registry doesn't drop
    // out from under us.
    drop(mutator);
}

// ── add-gc-safepoint-auto-threshold Phase 5 (2026-05-20) ─────────────────────

#[test]
fn auto_collect_flag_drained_at_next_safepoint() {
    // Manually set the auto-collect flag, call check_safepoint, verify
    // the flag is drained AND gc_cycles incremented (proof that the
    // safepoint path ran a real collect_cycles).
    let ctx = VmContext::new();
    // add-gc-safepoint-counter-throttling (2026-05-21): bypass throttling
    // — force the first check_safepoint into the slow path.
    ctx.safepoint_skip.store(1, Ordering::Relaxed);
    assert!(!ctx.core.needs_auto_collect.load(Ordering::Acquire));
    let cycles_before = ctx.heap().stats().gc_cycles;

    ctx.core.needs_auto_collect.store(true, Ordering::Release);
    check_safepoint(&ctx);

    assert!(!ctx.core.needs_auto_collect.load(Ordering::Acquire),
        "flag should have been swapped to false by check_safepoint");
    let cycles_after = ctx.heap().stats().gc_cycles;
    assert!(cycles_after > cycles_before,
        "expected gc_cycles to increment after drain (before={cycles_before}, after={cycles_after})");
}

#[test]
fn auto_collect_flag_idempotent_only_first_swap_runs_collect() {
    // Two consecutive check_safepoint calls with the flag set once: the
    // first drains and collects; the second sees false and is a fast no-op.
    let ctx = VmContext::new();
    ctx.core.needs_auto_collect.store(true, Ordering::Release);
    let cycles_before = ctx.heap().stats().gc_cycles;

    // add-gc-safepoint-counter-throttling (2026-05-21): force slow path
    // for each of the two check_safepoint calls below.
    ctx.safepoint_skip.store(1, Ordering::Relaxed);
    check_safepoint(&ctx);
    let cycles_after_first = ctx.heap().stats().gc_cycles;
    assert_eq!(cycles_after_first, cycles_before + 1);

    // Flag is false now; this call should NOT trigger another collect.
    ctx.safepoint_skip.store(1, Ordering::Relaxed);
    check_safepoint(&ctx);
    let cycles_after_second = ctx.heap().stats().gc_cycles;
    assert_eq!(cycles_after_second, cycles_after_first,
        "second check_safepoint should not collect when flag is false");
}

#[test]
fn request_pause_waits_for_other_mutators_to_park() {
    // collector + 2 mutator VmContexts. Spawn 2 worker threads that
    // immediately park (because we pre-set phase to Requested). Collector
    // calls request_gc_pause, which must wait for parked_count == 2.
    let collector = VmContext::new();
    let _m1 = VmContext::new_with_core(collector.core_arc());
    let _m2 = VmContext::new_with_core(collector.core_arc());

    let core1 = collector.core_arc();
    let core2 = collector.core_arc();

    // Pre-set phase to Requested so workers see it on first check.
    *collector.core.gc_phase.lock() = GcPhase::Requested;

    let w1 = std::thread::spawn(move || {
        let m = VmContext::new_with_core(core1);
        // add-gc-safepoint-counter-throttling (2026-05-21): force slow path.
        m.safepoint_skip.store(1, Ordering::Relaxed);
        check_safepoint(&m);
    });
    let w2 = std::thread::spawn(move || {
        let m = VmContext::new_with_core(core2);
        m.safepoint_skip.store(1, Ordering::Relaxed);
        check_safepoint(&m);
    });

    // Wait until both workers are parked (parked_count == 2). Note: the
    // long-lived _m1 / _m2 do NOT increment parked_count (they never
    // entered check_safepoint); only the threads' fresh VmContexts do.
    let start = std::time::Instant::now();
    while collector.core.parked_count.load(Ordering::Acquire) < 2 {
        std::thread::yield_now();
        if start.elapsed() > std::time::Duration::from_secs(5) {
            panic!("workers never both parked");
        }
    }

    // Release.
    *collector.core.gc_phase.lock() = GcPhase::Idle;
    collector.core.gc_phase_cv.notify_all();
    w1.join().expect("w1 panicked");
    w2.join().expect("w2 panicked");
}

// ── add-gc-safepoint-counter-throttling Phase 3 (2026-05-21) ─────────────────

#[test]
fn throttle_n_default_is_at_least_1024() {
    // Default (no env override) should be >= 1024. Cargo-test runs may
    // inherit an env from CI, so we don't assert exact equality — just
    // the default-or-larger floor. If Z42_SAFEPOINT_THROTTLE is set in
    // the environment to a smaller value, this test may show that.
    let n = throttle_n();
    assert!(n >= 1, "throttle must be at least 1 (every call slow path)");
}

#[test]
fn check_safepoint_fast_path_decrements_counter() {
    // 5 fast-path calls decrement the counter by 5 without taking the
    // slow path (gc_cycles unchanged).
    let ctx = VmContext::new();
    let initial = ctx.safepoint_skip.load(Ordering::Relaxed);
    let cycles_before = ctx.heap().stats().gc_cycles;

    // Skip this test if throttle is 1 (every call is slow path — the
    // `decrement by N` invariant collapses).
    if initial < 6 {
        eprintln!("skipping fast-path test under throttle={initial}; default 1024 required");
        return;
    }

    for _ in 0..5 {
        check_safepoint(&ctx);
    }

    let after = ctx.safepoint_skip.load(Ordering::Relaxed);
    assert_eq!(after, initial - 5, "counter should decrement by 5");
    assert_eq!(ctx.heap().stats().gc_cycles, cycles_before,
        "fast path should not run any collect");
}

#[test]
fn check_safepoint_slow_path_runs_when_counter_drains() {
    // Manually set counter to 1; next check_safepoint hits slow path
    // and resets counter back to `throttle_n()`.
    let ctx = VmContext::new();
    ctx.core.needs_auto_collect.store(true, Ordering::Release);
    ctx.safepoint_skip.store(1, Ordering::Relaxed);
    let cycles_before = ctx.heap().stats().gc_cycles;

    check_safepoint(&ctx);

    let after = ctx.safepoint_skip.load(Ordering::Relaxed);
    let expected_reset = throttle_n();
    assert_eq!(after, expected_reset,
        "slow path should reset counter to throttle_n() ({expected_reset}), got {after}");
    // The slow path should have observed + drained the auto_collect flag.
    assert!(!ctx.core.needs_auto_collect.load(Ordering::Acquire),
        "slow path should have drained needs_auto_collect");
    assert!(ctx.heap().stats().gc_cycles > cycles_before,
        "slow path should have run a real collect");
}

#[test]
fn check_safepoint_slow_path_pure_idle_no_op_resets_counter() {
    // No pending GC + no auto_collect; slow path is reached but does
    // nothing. Still resets the counter — proves the reset is unconditional.
    let ctx = VmContext::new();
    ctx.safepoint_skip.store(1, Ordering::Relaxed);
    let cycles_before = ctx.heap().stats().gc_cycles;

    check_safepoint(&ctx);

    let after = ctx.safepoint_skip.load(Ordering::Relaxed);
    assert_eq!(after, throttle_n());
    assert_eq!(ctx.heap().stats().gc_cycles, cycles_before,
        "Idle slow path should not run any collect");
}

// ── add-multi-collector-arbitration Phase 4 (2026-05-21) ─────────────────────

#[test]
fn request_gc_pause_returns_some_when_uncontested() {
    // No other collector active — CAS succeeds, returns Some.
    let ctx = VmContext::new();
    assert!(!ctx.core.collector_active.load(Ordering::Acquire));

    let guard = request_gc_pause(&ctx);
    assert!(guard.is_some(), "uncontested CAS should succeed");
    assert!(ctx.core.collector_active.load(Ordering::Acquire));
    drop(guard);
    // After Drop, collector_active released.
    assert!(!ctx.core.collector_active.load(Ordering::Acquire));
}

#[test]
fn second_collector_falls_back_to_mutator_park_returns_none() {
    // Pre-set collector_active = true to simulate another collector
    // holding the role. Spawn a worker thread that calls
    // request_gc_pause — it should park (briefly until we release the
    // bool and notify) and return None.
    let collector = VmContext::new();
    let _other = VmContext::new_with_core(collector.core_arc());

    // Simulate "another collector is active":
    collector.core.collector_active.store(true, Ordering::Release);
    *collector.core.gc_phase.lock() = GcPhase::Marking;

    let core = collector.core_arc();
    let worker = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        // GcPauseGuard borrows from w; we can't return it across the
        // thread boundary (w would drop first). Instead check is_some,
        // drop guard (if any) inside the closure, return the bool.
        let result = request_gc_pause(&w);
        let got_some = result.is_some();
        drop(result);
        got_some
    });

    // Wait until the worker has parked.
    let start = std::time::Instant::now();
    while collector.core.parked_count.load(Ordering::Acquire) < 1 {
        std::thread::yield_now();
        if start.elapsed() > std::time::Duration::from_secs(5) {
            panic!("worker never parked as fallback");
        }
    }

    // Release: clear collector_active + phase = Idle + notify.
    collector.core.collector_active.store(false, Ordering::Release);
    *collector.core.gc_phase.lock() = GcPhase::Idle;
    collector.core.gc_phase_cv.notify_all();

    let got_some = worker.join().expect("worker panicked");
    assert!(!got_some,
        "losing collector should return None, got Some");
    assert_eq!(collector.core.parked_count.load(Ordering::Acquire), 0);
}

// ── add-concurrent-gc P1 (2026-05-22) ────────────────────────────────────

#[test]
fn concurrent_marking_phase_does_not_park_mutators() {
    // Set phase to ConcurrentMarking and run check_safepoint repeatedly.
    // Mutators must NOT park — that's the entire point of the concurrent
    // path. Only Requested / Marking park.
    let ctx = VmContext::new();
    *ctx.core.gc_phase.lock() = GcPhase::ConcurrentMarking;

    // Force the slow path on every call.
    for _ in 0..100 {
        ctx.safepoint_skip.store(1, Ordering::Relaxed);
        check_safepoint(&ctx);
    }

    assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 0,
        "mutator must NOT park during ConcurrentMarking phase");
    assert_eq!(*ctx.core.gc_phase.lock(), GcPhase::ConcurrentMarking,
        "phase unchanged after safepoint checks");

    // Reset so the VmContext drop is clean.
    *ctx.core.gc_phase.lock() = GcPhase::Idle;
}

#[test]
fn requested_phase_still_parks_mutators_after_concurrent_added() {
    // Sanity: adding ConcurrentMarking variant must not regress the STW
    // path. Requested + Marking still park mutators (handled by spawn-
    // and-wait pattern from pause_guard_drop_notifies_waiters).
    let collector = VmContext::new();
    let _mutator = VmContext::new_with_core(collector.core_arc());

    *collector.core.gc_phase.lock() = GcPhase::Requested;

    let core = collector.core_arc();
    let handle = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        w.safepoint_skip.store(1, Ordering::Relaxed);
        check_safepoint(&w);
        // If parking worked, we land here only after collector flips to Idle.
    });

    // Wait until worker parks.
    let start = std::time::Instant::now();
    while collector.core.parked_count.load(Ordering::Acquire) < 1 {
        std::thread::yield_now();
        if start.elapsed() > std::time::Duration::from_secs(2) {
            panic!("mutator never parked under Requested phase");
        }
    }

    *collector.core.gc_phase.lock() = GcPhase::Idle;
    collector.core.gc_phase_cv.notify_all();
    handle.join().expect("worker panicked");
}

// ── add-repl-prewarm (2026-07-29): native-park guards ────────────────────────

#[test]
fn native_park_guard_satisfies_collector_without_safepoint() {
    // The crux of REPL prewarm: a thread blocked in a native call (REPL
    // readline) enters NativeParkGuard and NEVER calls check_safepoint, yet a
    // background collector must still be able to pause. `parked` simulates that
    // main thread; the collector's request_gc_pause (need = 2 ctxs - 1 = 1)
    // must proceed immediately, satisfied by the native-parked ctx.
    let collector = VmContext::new();
    let parked = VmContext::new_with_core(collector.core_arc());

    let guard = NativeParkGuard::enter(&parked);
    assert_eq!(collector.core.parked_count.load(Ordering::Acquire), 1,
        "entering the native-park guard must count toward parked_count");

    // Would hang forever if the native-parked ctx did NOT count (the collector
    // would wait for `parked` to reach a safepoint, which it never will).
    let g = request_gc_pause(&collector)
        .expect("uncontended CAS should succeed");
    assert_eq!(*collector.core.gc_phase.lock(), GcPhase::Marking,
        "collector should reach Marking without waiting on the native-parked ctx");
    drop(g); // world → Idle

    drop(guard); // Idle phase → no wait → parked_count back to 0
    assert_eq!(collector.core.parked_count.load(Ordering::Acquire), 0);
    drop(parked);
}

#[test]
fn native_unpark_guard_round_trips_parked_count() {
    // The completer fires mid-readline (inside a NativeParkGuard region) and
    // must temporarily un-park to run z42, then re-park. Single-threaded, Idle
    // throughout, so no waits — purely the increment/decrement balance.
    let ctx = VmContext::new();
    let park = NativeParkGuard::enter(&ctx);
    assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 1);
    {
        let _unpark = NativeUnparkGuard::exit(&ctx);
        assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 0,
            "un-park must drop the parked count so the completer runs as a mutator");
    }
    assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 1,
        "leaving the completer must re-park");
    drop(park);
    assert_eq!(ctx.core.parked_count.load(Ordering::Acquire), 0);
}

#[test]
fn release_re_enables_next_collector() {
    // After the active collector drops its guard, a fresh
    // request_gc_pause from the same thread should succeed.
    let ctx = VmContext::new();

    {
        let g1 = request_gc_pause(&ctx).expect("first claim");
        assert!(ctx.core.collector_active.load(Ordering::Acquire));
        drop(g1);
    }
    // After drop, collector_active = false. Next claim succeeds.
    assert!(!ctx.core.collector_active.load(Ordering::Acquire));
    let g2 = request_gc_pause(&ctx);
    assert!(g2.is_some(), "second claim after release should succeed");
}

// extend-runtime-counters P1a: a collection driven through the safepoint path in
// GenerationalMarkSweep mode bumps `minor_collections` (young-gen), unlike the
// non-generational path which only bumps `major_collections`.
#[test]
fn generational_collection_bumps_minor_counter() {
    let ctx = VmContext::new();
    ctx.heap().set_mode(crate::gc::GcMode::GenerationalMarkSweep);
    assert_eq!(ctx.heap().stats().minor_collections, 0);

    // Drive one real collection via the auto-collect safepoint drain, which
    // routes to collect_cycles_with_context → the generational minor path.
    ctx.safepoint_skip.store(1, Ordering::Relaxed);
    ctx.core.needs_auto_collect.store(true, Ordering::Release);
    check_safepoint(&ctx);

    let s = ctx.heap().stats();
    assert!(s.minor_collections >= 1,
        "generational collection should bump minor_collections (got {})", s.minor_collections);
    assert!(s.gc_cycles >= 1, "gc_cycles must also increment");
}
