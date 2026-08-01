//! GC safepoint protocol (add-gc-safepoint, 2026-05-20).
//!
//! Cooperative polling safepoint for the interp dispatch loop. Mutators
//! call [`check_safepoint`] at strategic points (function entry, backward
//! branches, Call return). The GC driver calls [`request_gc_pause`] which
//! blocks until every other `VmContext` has parked, runs mark+sweep while
//! holding the returned [`GcPauseGuard`], then drops the guard to release
//! everyone.
//!
//! State machine:
//!
//! ```text
//! Idle ──(request_gc_pause)──▶ Requested ──(all parked)──▶ Marking
//!   ▲                                                          │
//!   └────────────(GcPauseGuard::drop)────────────────────────  │
//! ```
//!
//! Mutators sleep on `gc_phase_cv` until phase returns to `Idle`. The
//! collector also sleeps on the same Condvar while waiting for `parked_count`
//! to reach `vm_contexts.len() - 1` (collector itself is excluded). The
//! collector re-reads `vm_contexts.len()` on each wakeup so a new VmContext
//! registered mid-pause doesn't strand the collector.
//!
//! v0 scope: interp only. JIT-compiled code lacks the Rust-level instrumentation
//! point — covered by follow-up `add-gc-safepoint-jit` (see Decision 5 in
//! `docs/spec/archive/2026-05-20-add-gc-safepoint/design.md`).

use crate::vm_context::VmContext;
use std::sync::atomic::Ordering;

/// add-gc-safepoint-counter-throttling (2026-05-21): default throttle
/// constant lives in `RuntimeConfig::safepoint_throttle` (defaults 1024
/// — mirrors HotSpot's polling-page heuristic; at z42's typical per-iter
/// cost ~50ns this caps GC pause latency at ≈ 50us, negligible vs actual
/// collect time 10ms+).
///
/// runtime-config-phase2 (2026-06-03): the OnceLock-cached env reader
/// moved into `RuntimeConfig` for centralised parsing + warnings.

/// Effective safepoint throttle. Reads from process-wide [`runtime_config()`]
/// (parsed once at first access; cached). Invalid values fall back to 1024
/// with a stderr warning at config init.
///
/// Setting `Z42_SAFEPOINT_THROTTLE=1` disables throttling (every call
/// runs the slow path) — useful for debugging latency-sensitive paths.
pub fn throttle_n() -> u32 {
    crate::config::runtime_config().safepoint_throttle
}

/// Current GC phase observed by mutators at safepoint checks.
///
/// **add-concurrent-gc P1 (2026-05-22)**: extended with `ConcurrentMarking`.
///
/// State machines:
///
/// ```text
/// STW path (GcMode::StwMarkSweep, default):
///   Idle ─►Requested─►Marking─►Idle
///                       ▲
///                       │ (mutators parked throughout Marking)
///
/// Concurrent path (GcMode::ConcurrentMarkSweep, opt-in):
///   Idle ─►Requested─►ConcurrentMarking─►Marking─►Idle
///                            ▲              ▲
///                            │              │ (short STW handshake
///                            │              │  for queue drain + sweep)
///                            │
///                       (mutators RUN; write barriers
///                        push gray refs to mark queue)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcPhase {
    /// No GC in progress; mutators run normally.
    Idle,
    /// Collector has requested a pause; mutators must park at the next safepoint.
    Requested,
    /// STW phase — collector is doing mark+sweep (default path) or the
    /// termination handshake + sweep (concurrent path); mutators parked.
    Marking,
    /// **add-concurrent-gc P1 (2026-05-22)**: concurrent mark phase —
    /// only set under `GcMode::ConcurrentMarkSweep`. Mutators continue
    /// executing during this phase (the write-barrier override is
    /// responsible for shading gray new refs). Transitions to `Marking`
    /// when the collector requests the final STW handshake.
    ///
    /// `check_safepoint_slow` explicitly does NOT park mutators when
    /// this phase is observed — that's the entire point of the
    /// concurrent path. Other phases (Requested / Marking) keep their
    /// STW parking semantics.
    ConcurrentMarking,
}

/// Fast-path safepoint check called from interp hot path.
///
/// **add-gc-safepoint-counter-throttling (2026-05-21)**: the Mutex-lock +
/// phase-check + auto-collect-drain logic only runs every [`throttle_n()`] th
/// call (default 1024). Worker liveness under a GC request is bounded by N
/// iterations × per-iter cost — at typical z42 hot-loop iter (~50ns) this caps
/// GC pause latency at ~50us, far below actual collect time.
///
/// **inline-jit-safepoint-check (2026-08-01)**: the fast path is a plain
/// `load + store` decrement (NOT `fetch_sub`). `safepoint_skip` is
/// **single-writer per mutator** — only the owning thread reads/writes it in
/// production; the sole cross-thread writer is [`VmContext::force_safepoint`],
/// which is test/embedder-only. So the read-modify-write atomicity is
/// unnecessary for correctness, and dropping it lets the JIT inline this fast
/// path as two bare `mov`s (see `jit::translate::emit_safepoint_check`) instead
/// of a helper call — the RMW form couldn't inline (`atomic_rmw` panicked on
/// x86_64) and cost a `LOCK`-prefixed instruction per hot-loop back-edge.
/// A missed cross-thread `force_safepoint` poke is bounded by throttle N, the
/// same latency ceiling the throttle already imposes.
#[inline]
pub fn check_safepoint(ctx: &VmContext) {
    // Fast path: plain (non-RMW) relaxed decrement. If the counter was > 1
    // before, we still have work to do before probing the real state.
    let prev = ctx.safepoint_skip.load(Ordering::Relaxed);
    ctx.safepoint_skip.store(prev.wrapping_sub(1), Ordering::Relaxed);
    if prev > 1 {
        return;
    }
    // Slow path: counter just hit 0 (or wrapped to u32::MAX in a
    // theoretical overflow — saturating reset below restores invariant).
    ctx.safepoint_skip.store(throttle_n(), Ordering::Relaxed);
    check_safepoint_slow(ctx);
}

/// Slow-path safepoint check — Mutex lock + phase check + auto-collect
/// drain. Called from [`check_safepoint`] every Nth call (per
/// [`throttle_n`]).
///
/// **add-gc-safepoint-auto-threshold (2026-05-20)**: when phase is Idle
/// but the heap's pressure-trip path has set `needs_auto_collect = true`,
/// the calling thread atomically claims the collect round via `swap(false,
/// AcqRel)` and runs a stop-the-world collect under [`request_gc_pause`].
/// If multiple threads see the flag, only the first swap-true claims;
/// the rest see false and skip (subsequent allocs that still trip pressure
/// re-set the flag).
///
/// **inline-jit-safepoint-check (2026-08-01)**: `pub(crate)` so the JIT's
/// `jit_check_safepoint_slow` helper (the rare slow branch of the inlined
/// fast path) can call it directly after resetting the throttle counter.
#[inline(never)]
pub(crate) fn check_safepoint_slow(ctx: &VmContext) {
    let phase = *ctx.core.gc_phase.lock();
    // add-concurrent-gc P1: `ConcurrentMarking` is observable but mutators
    // do NOT park — concurrent mark requires mutators to keep running so
    // the background mark thread isn't the only one making progress. The
    // write-barrier override (P3) handles tricolor shading on writes.
    if matches!(phase, GcPhase::Requested | GcPhase::Marking) {
        park_until_idle(ctx);
        return;
    }
    // Idle phase — drain pending auto-collect if any.
    if ctx.core.needs_auto_collect.swap(false, Ordering::AcqRel) {
        // add-concurrent-gc P4b (2026-05-22): use collect_cycles_with_context
        // so the heap can pick STW vs concurrent path internally. The STW
        // default impl does the same `request_gc_pause` + `collect_cycles`
        // dance as before; ArcMagrGC's override routes ConcurrentMarkSweep
        // mode through the multi-phase flow (snapshot → yield → drain →
        // handshake → sweep).
        ctx.heap().collect_cycles_with_context(ctx);
    }
}

/// Slow path — the mutator parks on the Condvar until the collector
/// releases the world. Releases on `Idle` *or* `ConcurrentMarking`
/// (add-concurrent-gc P1): the concurrent path transitions
/// `Requested → ConcurrentMarking` to signal mutators may resume; only
/// the final STW handshake (`Marking`) re-parks them.
fn park_until_idle(ctx: &VmContext) {
    ctx.core.parked_count.fetch_add(1, Ordering::AcqRel);
    // Acquire the phase lock BEFORE calling notify_all.
    //
    // parking_lot::Condvar does NOT buffer notifications: if notify_all
    // is sent when no thread is sleeping in wait(), the wake is lost.
    // Sending without the lock opens this window:
    //
    //   Worker: fetch_add → notify_all (no lock) → [blocked on lock]
    //   Collector: [checks condition — unsatisfied] → wait()  ← sleeps forever
    //
    // By acquiring the lock first, either:
    //   (a) Collector is in wait() (lock released) → we acquire it, notify,
    //       wake the collector correctly.
    //   (b) Collector holds the lock (in its loop body) → we block here.
    //       The collector will next call wait() or break. If it breaks
    //       (condition satisfied), our block ends when it releases the lock
    //       and we enter the wait loop, which exits immediately (Idle).
    //       If it calls wait(), we acquire the lock, notify, and wake it.
    //
    // In both cases the notification is never lost.
    let mut phase = ctx.core.gc_phase.lock();
    ctx.core.gc_phase_cv.notify_all();
    while matches!(*phase, GcPhase::Requested | GcPhase::Marking) {
        ctx.core.gc_phase_cv.wait(&mut phase);
    }
    // Decrement BEFORE releasing the phase lock. This closes a second race
    // with request_handshake_pause: decrementing after drop(phase) lets the
    // collector observe a stale elevated parked_count and break out of its
    // wait loop while this thread is still resuming (between drop and
    // fetch_sub). Decrementing under the lock serializes the count update
    // against the collector's next re-check.
    ctx.core.parked_count.fetch_sub(1, Ordering::AcqRel);
    drop(phase);
}

// ── add-repl-prewarm (2026-07-29): GC-safe park around a blocking native call ──
//
// A mutator that blocks in a native call (REPL rustyline `readline`) never
// reaches a bytecode safepoint, so a background collector on another thread
// would wait forever for it to park. These helpers let such a thread count as
// "parked" for the whole blocking span — its z42 roots are frozen while it sits
// in native code, so the collector can scan them safely (the classic
// JVM `_thread_in_native` / Go `entersyscall` transition). Same `parked_count`
// + `gc_phase_cv` machinery as `park_until_idle`; no new synchronization.

/// Enter the parked state: count this ctx toward `parked_count` and wake any
/// collector waiting for its target. Caller must NOT mutate z42 roots or
/// allocate until the matching [`native_park_decr`].
fn native_park_incr(ctx: &VmContext) {
    ctx.core.parked_count.fetch_add(1, Ordering::AcqRel);
    // Hold the phase lock across notify_all — same lost-wakeup discipline as
    // park_until_idle: a collector spinning in its wait loop must observe our
    // increment.
    let _phase = ctx.core.gc_phase.lock();
    ctx.core.gc_phase_cv.notify_all();
}

/// Leave the parked state. If a STW window is in progress, wait it out BEFORE
/// resuming mutation (else we'd race the collector scanning our roots), then
/// drop our parked count. Decrement under the phase lock closes the same
/// `request_handshake_pause` race documented in `park_until_idle`.
fn native_park_decr(ctx: &VmContext) {
    let mut phase = ctx.core.gc_phase.lock();
    while matches!(*phase, GcPhase::Requested | GcPhase::Marking) {
        ctx.core.gc_phase_cv.wait(&mut phase);
    }
    ctx.core.parked_count.fetch_sub(1, Ordering::AcqRel);
    drop(phase);
}

/// RAII: marks the calling `VmContext` GC-safe for the duration of a blocking
/// native call. Wrap the outermost native read (`builtin_repl_readline` /
/// `builtin_repl_readblock`) so a background prewarm thread's GC can proceed
/// while the main thread blocks on stdin. Drop restores the running-mutator
/// state, waiting out any in-flight STW pause first.
pub struct NativeParkGuard<'a> {
    ctx: &'a VmContext,
}

impl<'a> NativeParkGuard<'a> {
    pub fn enter(ctx: &'a VmContext) -> Self {
        native_park_incr(ctx);
        NativeParkGuard { ctx }
    }
}

impl Drop for NativeParkGuard<'_> {
    fn drop(&mut self) {
        native_park_decr(self.ctx);
    }
}

/// RAII inverse of [`NativeParkGuard`]: temporarily leaves the parked state so a
/// ctx already inside a `NativeParkGuard` region can re-enter the VM. Used for
/// the REPL Tab-completer, which rustyline fires synchronously from inside the
/// blocking `readline` — the completer runs z42 as a normal mutator (parking at
/// its own safepoints if a GC is requested), then Drop re-parks for the
/// remaining blocking read.
pub struct NativeUnparkGuard<'a> {
    ctx: &'a VmContext,
}

impl<'a> NativeUnparkGuard<'a> {
    pub fn exit(ctx: &'a VmContext) -> Self {
        native_park_decr(ctx);
        NativeUnparkGuard { ctx }
    }
}

impl Drop for NativeUnparkGuard<'_> {
    fn drop(&mut self) {
        native_park_incr(self.ctx);
    }
}

/// RAII guard returned by [`request_gc_pause`]. While held, the collector
/// is in the `Marking` phase and all *other* VmContexts are parked. Drop
/// releases everyone.
pub struct GcPauseGuard<'a> {
    ctx: &'a VmContext,
}

/// Collector-side entry. Transitions `Idle → Requested`, waits for every
/// other live VmContext to park, then transitions `Requested → Marking`
/// and returns the guard. Caller does mark+sweep, then drops the guard to
/// transition `Marking → Idle` and notify all parked mutators.
///
/// **add-multi-collector-arbitration (2026-05-21)**: returns
/// `Option<GcPauseGuard>`. The leading CAS on `collector_active` ensures
/// only one thread can be the active collector at a time:
///
/// - `Some(guard)` — we claimed the collector role; caller proceeds with
///   `collect_cycles()` / `force_collect()`
/// - `None` — another collector is active. We've already parked-as-mutator
///   inside this call (contributing to the active collector's
///   `parked_count` target). Caller skips its collect.
///
/// The collector itself is **never** counted in `parked_count`; only other
/// VmContexts are waited for. If the collector is the only live VmContext
/// (`vm_contexts.len() == 1`), the wait condition `need_parked == 0` is
/// satisfied immediately.
pub fn request_gc_pause(ctx: &VmContext) -> Option<GcPauseGuard<'_>> {
    // Atomic CAS: claim the unique collector role. Acquire side pairs
    // with the previous collector's `Release` store in GcPauseGuard::drop
    // (so we see its heap changes); Release side pairs with our
    // subsequent `gc_phase = Requested` store (so workers seeing
    // Requested also see our collector_active = true).
    if ctx.core.collector_active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        // Another collector is active. Park-as-mutator so the active
        // collector's `parked_count` target is reached faster; return
        // None so caller skips its own collect.
        park_until_idle(ctx);
        return None;
    }

    *ctx.core.gc_phase.lock() = GcPhase::Requested;

    // Wait for everyone-but-self to park. Re-read vm_contexts.len() on
    // each wakeup so a freshly-registered VmContext (which will see
    // Requested at its first safepoint check and park itself) doesn't
    // strand us with a stale threshold.
    let mut phase = ctx.core.gc_phase.lock();
    loop {
        let total = ctx.core.vm_contexts.lock().len();
        let need  = total.saturating_sub(1);
        if ctx.core.parked_count.load(Ordering::Acquire) >= need {
            break;
        }
        ctx.core.gc_phase_cv.wait(&mut phase);
    }
    *phase = GcPhase::Marking;
    drop(phase);

    Some(GcPauseGuard { ctx })
}

impl Drop for GcPauseGuard<'_> {
    fn drop(&mut self) {
        *self.ctx.core.gc_phase.lock() = GcPhase::Idle;
        self.ctx.core.gc_phase_cv.notify_all();
        // add-multi-collector-arbitration (2026-05-21): release the
        // exclusive collector claim. Release ordering so the next
        // collector's compare_exchange Acquire sees our final heap state.
        self.ctx.core.collector_active.store(false, Ordering::Release);
    }
}

// ── add-concurrent-gc P4b (2026-05-22) ────────────────────────────────────
//
// Phase transition methods on the held guard. Used by the concurrent
// collect loop:
//   1. request_gc_pause → guard in Marking phase, mutators parked
//   2. yield_to_concurrent_marking → guard in ConcurrentMarking phase,
//      mutators wake from park_until_idle and resume (write barriers
//      shade gray writes per add-concurrent-gc P3)
//   3. background mark drain by collector thread (this thread)
//   4. request_handshake_pause → guard back to Marking phase, waits
//      for all other VmContexts to re-park at their next safepoint
//   5. (collector drains residual gray, runs STW sweep)
//   6. drop → release everyone

impl GcPauseGuard<'_> {
    /// Transition `Marking → ConcurrentMarking`. Mutators waiting on
    /// `gc_phase_cv` wake (their wait predicate `Requested | Marking`
    /// no longer matches) and resume execution. The collector retains
    /// its `collector_active` claim — no other collector can preempt.
    ///
    /// Caller must currently be in `Marking` phase (asserted in
    /// debug builds); typical caller acquired guard via
    /// `request_gc_pause`.
    pub fn yield_to_concurrent_marking(&self) {
        let mut phase = self.ctx.core.gc_phase.lock();
        debug_assert_eq!(*phase, GcPhase::Marking,
            "yield_to_concurrent_marking expects current phase Marking");
        *phase = GcPhase::ConcurrentMarking;
        drop(phase);
        self.ctx.core.gc_phase_cv.notify_all();
    }

    /// Transition `ConcurrentMarking → Marking`. Waits for all other
    /// VmContexts to park at their next safepoint check (same wait
    /// pattern as `request_gc_pause`). After return, the world is
    /// STW-stopped exactly as after `request_gc_pause` (mutators
    /// parked, mark queue safe to drain without race).
    pub fn request_handshake_pause(&self) {
        let mut phase = self.ctx.core.gc_phase.lock();
        debug_assert_eq!(*phase, GcPhase::ConcurrentMarking,
            "request_handshake_pause expects current phase ConcurrentMarking");
        *phase = GcPhase::Marking;
        // Wake mutators currently in safepoint slow-path checks so they
        // observe the new phase + park. New ones hitting safepoint will
        // see Marking and park directly.
        self.ctx.core.gc_phase_cv.notify_all();
        // Wait for everyone-but-self to park. Re-read vm_contexts.len()
        // on each wakeup so a freshly-registered VmContext doesn't
        // strand us.
        loop {
            let total = self.ctx.core.vm_contexts.lock().len();
            let need  = total.saturating_sub(1);
            if self.ctx.core.parked_count.load(Ordering::Acquire) >= need {
                break;
            }
            self.ctx.core.gc_phase_cv.wait(&mut phase);
        }
    }
}

#[cfg(test)]
#[path = "safepoint_tests.rs"]
mod safepoint_tests;
