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

use crate::vm_context::{VmContext, VmCore};
use std::sync::atomic::Ordering;

/// How long a collector waits for stragglers before poking every unparked mutator again
/// (see [`poke_safepoints`]).
const REPOKE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1);

/// **P0-16 (gc-strategy-stopgap)**: send every registered mutator's **next** `check_safepoint`
/// down the slow path, by resetting its throttle counter to 1 — the cross-thread twin of
/// [`VmContext::force_safepoint`].
///
/// Called when a collection is requested (the auto-collect trip raises `needs_auto_collect`) and
/// when a collector asks for the pause. Without it each mutator only looks at either after its
/// throttle counter runs out — up to `Z42_SAFEPOINT_THROTTLE` checks later, per thread, on top of
/// each other.
///
/// The counter is single-writer by design (the fast path is a plain load + store, see
/// [`check_safepoint`]), so a poke can be overwritten by a mutator that loaded the counter just
/// before it. That is why the collector re-pokes while it waits ([`REPOKE_INTERVAL`]); a lost poke
/// at the trip costs at most the throttle, as before, because the request itself is sticky.
pub(crate) fn poke_safepoints(core: &VmCore) {
    // SAFETY: see `VmContextPtr` — an entry is live while it is registered, and we hold the
    // registry lock for the walk. The counter is an atomic.
    for p in core.vm_contexts.lock().iter() {
        unsafe { &*p.0 }.safepoint_skip.store(1, Ordering::Relaxed);
    }
}

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
/// Every pause — a STW collect, a minor, one slice of an incremental major —
/// has the same shape:
///
/// ```text
///   Idle ─►Requested─►Marking─►Idle
///                       ▲
///                       │ (mutators parked throughout Marking)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcPhase {
    /// No GC in progress; mutators run normally.
    Idle,
    /// Collector has requested a pause; mutators must park at the next safepoint.
    Requested,
    /// The collector holds the pause and does its work; mutators parked.
    Marking,
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
/// the calling thread runs a stop-the-world collect under [`request_gc_pause`].
/// If multiple threads see the flag, the collector-role CAS in
/// `request_gc_pause` picks one; the rest park as mutators. The flag stays set
/// until the winner holds the pause (P0-16: sticky — see `take_collect_request`).
///
/// **inline-jit-safepoint-check (2026-08-01)**: `pub(crate)` so the JIT's
/// `jit_check_safepoint_slow` helper (the rare slow branch of the inlined
/// fast path) can call it directly after resetting the throttle counter.
#[inline(never)]
pub(crate) fn check_safepoint_slow(ctx: &VmContext) {
    let phase = *ctx.core.gc_phase.lock();
    if matches!(phase, GcPhase::Requested | GcPhase::Marking) {
        park_until_idle(ctx);
        return;
    }
    // Idle phase — serve a pending auto-collect request if any. **Sticky** (P0-16): it is only
    // *read* here; the collector clears it once it holds the pause (`take_collect_request`), so a
    // thread that loses the collector role below leaves it set for the next safepoint instead of
    // consuming it.
    if ctx.core.needs_auto_collect.load(Ordering::Acquire) {
        // collect_cycles_with_context takes the pause (`request_gc_pause`)
        // and lets the heap pick the work for its mode.
        ctx.heap().collect_cycles_with_context(ctx);
    }

    // add-sampling-profiler (2026-08-24, script-profiling P2): when the
    // sampling profiler is on (Z42_SAMPLE_HZ set), snapshot the z42 call stack
    // if the background timer flagged a sample. `enabled()` is one atomic load;
    // default-off (the common case) short-circuits here with zero further work.
    // This runs only on the already-throttled slow path (Idle tail, no GC lock
    // held — gc_phase lock was a temporary above), so it never touches the hot
    // `check_safepoint` fast path.
    if ctx.core.sampler.enabled() {
        ctx.core.sampler.maybe_sample(ctx);
    }
}

/// Slow path — the mutator parks on the Condvar until the collector
/// releases the world (phase back to `Idle`).
fn park_until_idle(ctx: &VmContext) {
    // add-concurrency-probes (2026-08-23, script-profiling P1b): time how long
    // this mutator stays parked (STW stall as seen by the stopped thread). This
    // runs ONLY on the park slow path (an actual GC pause), never on the hot
    // `check_safepoint` fast path — so it's free to leave always-on.
    let park_start = std::time::Instant::now();
    // add-gc-tlab (stage 2, D5): retire this thread's TLAB BEFORE signalling
    // parked — the collector proceeds to mark/sweep once parked_count reaches
    // its target, and must see a fully-merged region with no chunk still
    // borrowed (mid-fill) by this thread. Retire takes the region locks briefly;
    // the collector is only waiting on parked_count here (holds no region lock),
    // so no deadlock. Idempotent when the TLAB is already unbound.
    ctx.heap().retire_thread_tlab();
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
    // Decrement BEFORE releasing the phase lock. Decrementing after
    // drop(phase) would let the next collector's `request_gc_pause` observe
    // a stale elevated parked_count and break out of its wait loop while
    // this thread is still resuming (between drop and fetch_sub).
    // Decrementing under the lock serializes the count update against the
    // collector's next re-check.
    ctx.core.parked_count.fetch_sub(1, Ordering::AcqRel);
    drop(phase);
    // Record park duration AFTER releasing the phase lock (park_histogram is a
    // distinct lock; recording here can't deadlock the collector). Saturating
    // µs cast is fine — a single park never approaches u64 µs.
    ctx.core
        .park_histogram
        .lock()
        .record(park_start.elapsed().as_micros() as u64);
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

thread_local! {
    /// **fix-alloc-inside-native-park (2026-09-14)**: how many [`NativeParkGuard`]s this
    /// thread is inside (net of [`NativeUnparkGuard`]s). Read only by
    /// [`debug_assert_not_native_parked`].
    static NATIVE_PARK_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// **fix-alloc-inside-native-park (2026-09-14)**: debug-build tripwire for the one rule a
/// parked thread must keep — **no allocation until the park ends**.
///
/// A parked thread counts as stopped, so a collection on another thread proceeds without
/// it. Anything this thread allocates in that window lives only in a Rust local: no frame
/// register, no pinned root. The collector sweeps it as garbage, and the dead-edge break
/// nulls its reference fields while the thread goes on to use it. Measured symptom:
/// `Z42NetHttpServerThreadedTests` failing 3 runs in 40 with `BrCond expects bool, got
/// Null` / `ArraySet index: expected non-negative integer, got Null` — both TCP connect
/// builtins built their result tuple before their park guard dropped.
///
/// The race is microseconds wide and never reproduces on demand, so the check is on the
/// rule rather than on its consequence: every allocation path (`record_alloc` and
/// `record_alloc_fast`) calls this. Release builds compile it out.
#[inline]
pub(crate) fn debug_assert_not_native_parked() {
    #[cfg(debug_assertions)]
    NATIVE_PARK_DEPTH.with(|d| {
        assert!(
            d.get() == 0,
            "GC allocation inside a NativeParkGuard region: a parked thread's new objects are \
             not GC roots, so a concurrent collection frees them. End the park (drop the \
             guard) before allocating — see gc/safepoint.rs"
        );
    });
}

/// Debug-build tripwire for the other half of that rule: a parked thread's frame stack is
/// read by the collector without synchronization (`FrameStack::scan_parked`), so the thread
/// must not push or pop frames until the park ends. Called by `FrameStack::push` / `pop`.
#[inline]
pub(crate) fn debug_assert_frame_change_not_parked() {
    #[cfg(debug_assertions)]
    NATIVE_PARK_DEPTH.with(|d| {
        assert!(
            d.get() == 0,
            "z42 frame pushed / popped inside a NativeParkGuard region: a collector may be \
             scanning this thread's frame stack. End the park (or use NativeUnparkGuard) \
             before running z42 code — see gc/safepoint.rs"
        );
    });
}

/// Enter the parked state: count this ctx toward `parked_count` and wake any
/// collector waiting for its target. Caller must NOT mutate z42 roots or
/// allocate until the matching [`native_park_decr`] — enforced in debug builds by
/// [`debug_assert_not_native_parked`].
fn native_park_incr(ctx: &VmContext) {
    // add-gc-tlab (stage 2, D5): retire this thread's TLAB before it counts as
    // parked for a blocking native call — a background collector may scan the
    // region while this thread sits in native code, so no chunk may stay
    // borrowed. (The thread won't allocate again until native_park_decr.)
    ctx.heap().retire_thread_tlab();
    NATIVE_PARK_DEPTH.with(|d| d.set(d.get() + 1));
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
/// stale-count race documented in `park_until_idle`.
fn native_park_decr(ctx: &VmContext) {
    let mut phase = ctx.core.gc_phase.lock();
    while matches!(*phase, GcPhase::Requested | GcPhase::Marking) {
        ctx.core.gc_phase_cv.wait(&mut phase);
    }
    ctx.core.parked_count.fetch_sub(1, Ordering::AcqRel);
    drop(phase);
    NATIVE_PARK_DEPTH.with(|d| d.set(d.get() - 1));
}

/// RAII: marks the calling `VmContext` GC-safe for the duration of a blocking
/// native call. Wrap the outermost native read (`builtin_repl_readline`) so a
/// background prewarm thread's GC can proceed while the main thread blocks on
/// stdin. Drop restores the running-mutator
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

    // add-gc-tlab (stage 2, D5): the collector itself may hold a borrowed chunk
    // from its own prior allocations. Retire it now, before marking — otherwise
    // its mid-fill chunk stays invisible to sweep (borrowed) and its objects'
    // mark bits would not be cleared for the next cycle. Other mutators retire
    // when they park (park_until_idle) below.
    ctx.heap().retire_thread_tlab();

    *ctx.core.gc_phase.lock() = GcPhase::Requested;

    // Wait for everyone-but-self to park. Re-read vm_contexts.len() on
    // each wakeup so a freshly-registered VmContext (which will see
    // Requested at its first safepoint check and park itself) doesn't
    // strand us with a stale threshold.
    //
    // P0-16: every mutator is poked into its slow path first, and again each
    // `REPOKE_INTERVAL` the wait goes on — a poke can be lost to a racing
    // decrement, and a newcomer registers with a full throttle counter.
    let mut phase = ctx.core.gc_phase.lock();
    loop {
        let total = ctx.core.vm_contexts.lock().len();
        let need  = total.saturating_sub(1);
        if ctx.core.parked_count.load(Ordering::Acquire) >= need {
            break;
        }
        poke_safepoints(&ctx.core);
        ctx.core.gc_phase_cv.wait_for(&mut phase, REPOKE_INTERVAL);
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

#[cfg(test)]
#[path = "safepoint_tests.rs"]
mod safepoint_tests;
