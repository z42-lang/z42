//! Automatic-collection policy: when allocation pressure trips a GC cycle.
//!
//! Split out of `alloc.rs` by add-gc-runtime-knobs (2026-09-05); rewritten by
//! **arm-gc-by-default (2026-09-09)** around Mono SGen's memory governor.
//!
//! ## The policy
//!
//! ```text
//!    alloc ──► used >= next_collect_at ? ──no──► nothing (one relaxed load)
//!                     │yes
//!               decide_trip()
//!     ┌────────────────┴─────────────────┐
//!     │ generational                     │ STW (one generation)
//!     │ promoted >= allowance  → major   │ used - live >= allowance → major
//!     │ else grown >= nursery  → minor   │
//!     └──────────────────────────────────┘
//!
//!    allowance(live) = MAX(live × 0.33, nursery × 4), then squeezed by any soft cap
//! ```
//!
//! **Every threshold is relative**, so nothing here needs a byte budget to exist first —
//! which is what lets the collector be armed by default. `Z42_GC_MAX_BYTES` is a **soft cap**
//! now, not the arming switch: unset means "no cap" (Mono's `soft_heap_limit` default), and
//! setting one only squeezes the allowance and adds a near-limit trip.
//!
//! Before this, unset meant *no automatic collection at all* — the historical default — so a
//! long-running program grew until it exited.
//!
//! ## Why the trip point is cached
//!
//! `maybe_auto_collect` reads watermarks out of the heap's `inner` mutex. Arming by default
//! would therefore put a lock acquire on the hottest path in the VM; the only thing keeping it
//! off before was "no budget ⇒ return immediately". [`ArcMagrGC::next_collect_at`] — Mono's
//! `major_collection_trigger_size` — caches the `used_bytes` reading at which the policy wants
//! to be consulted again, so an allocation costs one relaxed load and a compare, and the slow
//! path runs at most once per growth gate.
//!
//! ## Futility backoff
//!
//! A live set that genuinely exceeds its cap makes every collection reclaim ~nothing while the
//! heap keeps growing, so a gate that only asks for growth re-arms forever. Measured on
//! `src/tests/perf/scenarios/09_alloc_ctorless` with a 64MB budget: a 0.29s run had not finished
//! after 9 minutes, doing a 0-byte 75ms mark-sweep every ~6MB.
//!
//! A relative allowance already blunts this — the gate grows with the live set, making the
//! collection count logarithmic in heap growth rather than linear — so the backoff is now the
//! belt for the case a soft cap squeezes the allowance down to its floor. Each consecutive
//! *unproductive* collection doubles the growth required before trying again, capped at
//! [`MAX_BACKOFF`]; one productive collection resets it.
//!
//! **What counts as "unproductive" is deliberately narrow** (fix-futile-backoff-is-too-eager,
//! 2026-09-11): essentially-nothing reclaimed, not merely less than the gate. The multiplier
//! lands on the **nursery**, which is not a memory gate but the bound on one minor's pause, so
//! a bar set too high answers "this program has a high survival rate" with "then scan four
//! times as much next time". See [`ArcMagrGC::next_backoff`] for the 64.6 ms minor that cost.
//!
//! Productivity is read back from `stats.reclaimed_bytes` — the total is already maintained by
//! every collect path, so this needs no hook in any of them. It always describes the collection
//! the *previous* trip asked for, which leaves the very first trip with nothing to judge:
//! reading 0 reclaimed there penalised a heap that had never been collected at all, so
//! `gc_cycles == 0` means "neutral", not "futile".

use std::sync::atomic::Ordering;

/// Cap on the growth-gate multiplier. With the 0.10 default throttle ratio this
/// means a hopeless heap re-collects at most a handful more times as it grows,
/// instead of every 10% of the budget forever.
const MAX_BACKOFF: u32 = 64;

/// **fix-minor-and-major-in-one-pause (2026-09-10)**: a collection that reclaimed less than
/// `gate / FUTILE_DIVISOR` counts as having freed *nothing*, and backs off harder than one that
/// merely under-performed. 1/16 of a gate is far below anything a healthy collection returns
/// (a healthy minor reclaims most of a nursery) and far above the handful of bytes a
/// 100%-survival workload gives back, so the two cases never overlap in practice.
const FUTILE_DIVISOR: u64 = 16;

/// **arm-gc-by-default (2026-09-09)**: the unit the whole policy is denominated in — how much
/// may be allocated before a **minor**, and (times [`ALLOWANCE_NURSERY_RATIO`]) the floor
/// under a **major**'s allowance. Overridable via `Z42_GC_NURSERY_BYTES`.
///
/// It is the knob that buys a **pause bound**: a minor only scans the young set, so this is
/// how much young set one minor has to chew through. Mono SGen's default is 4 MB
/// (`SGEN_DEFAULT_NURSERY_SIZE = 1 << 22`); z42's is four times that.
///
/// **retune-gc-nursery-and-promotion-age (2026-09-11)**: 32M → **16M**, together with
/// `PROMOTION_THRESHOLD` 2 → 3. Measured across three workloads, two binaries, 3 runs each:
///
/// | | `09_alloc_ctorless` wall | `12_gc_churn` RSS / p90 | `z42c.semantics` wall / RSS / p90 |
/// |---|---|---|---|
/// | 32M age2 (before) | 0.38 s | 184 MB / 29.7 ms | 7.00 s / 628 MB / 23.6 ms |
/// | 24M age3 | 0.36 s | 186 MB / 21.0 ms | 6.91 s / 617 MB / 18.0 ms |
/// | **16M age3** | 0.45 s | **142 MB / 16.4 ms** | 7.19 s / **595 MB / 14.1 ms** |
///
/// Two things in that table are worth carrying forward.
///
/// **The promotion age is not optional baggage — it is what makes 16M affordable.** A minor
/// promotes whatever survives it, so halving the nursery halves how much allocation an object
/// must outlive to be promoted: objects that would have died young get moved to the old
/// generation instead, where only a major can reclaim them, and on a young-death workload a
/// major may never run. That is **premature promotion**, and it inverts the intuition that a
/// smaller nursery means a smaller footprint — measured on `12_gc_churn`, 16M at the old age
/// of 2 promoted **33.6%** of scanned objects against **17.9%** at 32M (1 326 948 vs 684 444,
/// for the same ~3.9 M scanned), and peak RSS went from 198 MB **up** to 405 MB. Age 3 takes
/// the same rung to 142 MB — lower than the 32M default it replaces.
///
/// **`09_alloc_ctorless` regresses ~18%, deliberately.** It is the 100%-survival pathology:
/// nothing it allocates ever dies, so every collection is waste, and a smaller nursery fits one
/// more of them in before [`ArcMagrGC::next_backoff`] saturates (2 collections at 24M, 3 at
/// 16M). 24M avoids it and improves every workload a little; 16M costs that one benchmark and
/// buys roughly twice the pause reduction everywhere else. **The trade was put to the project
/// owner explicitly and 16M was chosen** — this line is the pause line, and the regression is
/// on a synthetic whose defining property is that collecting it can never help.
pub(super) const DEFAULT_NURSERY_BYTES: u64 = 16 * 1024 * 1024;

/// **arm-gc-by-default (2026-09-09)**: fraction of the live set the old generation may take
/// in before the next major. Mono SGen's `SGEN_DEFAULT_ALLOWANCE_HEAP_SIZE_RATIO` = 0.33 —
/// "let the heap grow by one third before collecting it again".
const ALLOWANCE_HEAP_RATIO: f64 = 0.33;

/// **arm-gc-by-default (2026-09-09)**: floor on that allowance, in nurseries. Mono SGen's
/// `SGEN_DEFAULT_ALLOWANCE_NURSERY_SIZE_RATIO` = 4.0 — without a floor, a program with a
/// tiny live set would major-collect constantly (a third of "almost nothing" is nothing).
const ALLOWANCE_NURSERY_RATIO: u64 = 4;

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: when the
    /// `external_needs_collect` flag is wired (post-`VmCore` construction) this
    /// only does `flag.store(true, Release)` — the collect itself is deferred to
    /// the next mutator `check_safepoint(ctx)` and runs inside its safepoint
    /// guard, so the root scanner never races a mutator's `regs` writes. With no
    /// flag wired (GC unit tests constructing `ArcMagrGC::new()` directly) it
    /// falls back to the inline collect, leaving single-threaded behaviour
    /// unchanged.
    pub(super) fn maybe_auto_collect(&self) {
        // arm-gc-by-default: the cheap gate, before the `inner` mutex. Callers on the TLAB
        // fast path check this themselves; the locked allocation paths reach it here.
        let used = self.used_bytes_atomic();
        if used < self.next_collect_at.load(Ordering::Relaxed) {
            return;
        }
        let (soft_limit, last, paused, reclaimed_mark, backoff, total_reclaimed, cycles) = {
            let i = self.inner.lock();
            (i.stats.max_bytes, i.last_auto_collect_used, i.pause_count > 0,
             i.last_auto_collect_reclaimed,
             // `RcHeapInner` derives Default, so the multiplier starts at 0;
             // treat that as the neutral 1 rather than hand-writing a Default
             // impl for a 20-field struct (0 would zero the growth gate and
             // trip a collect on every allocation).
             i.auto_collect_backoff.max(1),
             i.stats.reclaimed_bytes,
             i.stats.gc_cycles)
        };
        if paused { return; }
        let cfg = crate::config::runtime_config();

        // How much did the *previous* trip's collection actually reclaim?
        let reclaimed_since = total_reclaimed.saturating_sub(reclaimed_mark);
        // `last` is a pre-collect reading; the collection it tripped then freed
        // `reclaimed_since`, so this is where that collection left the heap — the live set
        // it swept down to. Mono calls the same quantity `new_heap_size`, and the whole
        // allowance policy is expressed relative to it.
        let baseline = last.saturating_sub(reclaimed_since);

        let Some(trip) = self.decide_trip(used, baseline, backoff, soft_limit, &cfg) else {
            // Nothing to do — but re-arm, or the next allocation walks back in here and takes
            // the `inner` mutex all over again.
            self.arm_next_collect(baseline, used, backoff, soft_limit);
            return;
        };

        let next_backoff = Self::next_backoff(backoff, reclaimed_since, trip.gate, cycles);

        // A collect this path already asked for may still be pending at the
        // safepoint. Re-tripping would overwrite the watermarks with readings
        // taken before it ran, losing that cycle from the accounting — and
        // would not buy a second collection anyway, the flag is already set.
        let pending = self.external_needs_collect.lock().clone();
        if let Some(f) = &pending {
            if f.load(Ordering::Acquire) {
                self.arm_next_collect(baseline, used, backoff, soft_limit);
                return;
            }
        }
        // Hold off re-entry until the collection lands: `sub_used_bytes` re-arms from the
        // post-collection live set, which is the only reading that gives the right next gate.
        self.next_collect_at.store(u64::MAX, Ordering::Relaxed);

        {
            // Mark the pre-collect watermarks so we don't re-trip on every
            // subsequent alloc while the deferred collect is still pending.
            let mut i = self.inner.lock();
            i.last_auto_collect_used = used;
            i.last_auto_collect_reclaimed = total_reclaimed;
            i.auto_collect_backoff = next_backoff;
        }

        // `Z42_GC_PHASES`: why this collection is happening at all. The phase lines that
        // follow account for the pause; this one accounts for the *young set* the pause had to
        // chew through, which is decided here and nowhere else.
        crate::gc::phase_timer::note(format_args!(
            "trip {:<5}  gate {}{}  grown {}  (last freed {})",
            if trip.major { "major" } else { "minor" },
            crate::gc::trace::human(trip.gate),
            if backoff > 1 { format!(" x{backoff}") } else { String::new() },
            crate::gc::trace::human(used.saturating_sub(baseline)),
            crate::gc::trace::human(reclaimed_since),
        ));
        // Which kind of collection the policy is asking for. The deferred safepoint path only
        // knows "collect", so the choice is handed over through `pending_major`.
        if trip.major {
            self.pending_major.store(true, Ordering::Release);
        }

        // Defer to safepoint when wired (multi-thread safe path).
        if let Some(flag) = pending {
            flag.store(true, Ordering::Release);
            return;
        }
        // Fallback: legacy inline collect — preserves GC unit-test behaviour
        // (those tests construct ArcMagrGC::new() without VmCore wiring).
        self.collect_cycles();
    }
}

/// What [`ArcMagrGC::decide_trip`] decided: which kind of collection, and the growth gate it
/// tripped (the futility bar is read off the same number).
struct Trip {
    major: bool,
    gate: u64,
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// **arm-gc-by-default (2026-09-09)**: the auto-collect policy, modelled on Mono SGen's
    /// memory governor. The shape that matters: **every threshold is relative**, so the
    /// collector works without anyone naming a byte budget — which is what lets it be armed
    /// by default.
    ///
    /// - **minor** (generational only) trips on **allocation volume**: one nursery's worth
    ///   since the last collection. `Z42_GC_NURSERY_BYTES`, default
    ///   [`DEFAULT_NURSERY_BYTES`], an **absolute** figure (Mono's is 4 MB) — it bounds minor
    ///   work, which has nothing to do with how big the heap is allowed to get.
    /// - **major** trips on **old-generation growth** measured against the live set the last
    ///   major swept down to: `live × ALLOWANCE_HEAP_RATIO`, floored at
    ///   `nursery × ALLOWANCE_NURSERY_RATIO`. Under STW there is one generation, so the same
    ///   rule reads off `used` directly.
    /// - `Z42_GC_MAX_BYTES` is now a **soft cap, not the arming switch**: unset means "no
    ///   cap" (Mono's `soft_heap_limit` default), and when set it only squeezes the allowance
    ///   and adds a near-limit trip.
    /// How the futility multiplier should move, given what the **previous** collection returned
    /// against the gate this one tripped.
    ///
    /// Two tiers, not three (fix-futile-backoff-is-too-eager, 2026-09-11). The middle one —
    /// *"reclaimed less than **half** a gate ⇒ double the multiplier"* — was the original rule,
    /// and it cannot tell the two cases apart that matter:
    ///
    /// - **Genuinely futile**: the live set is not producing garbage at all, so the next
    ///   collection marks a whole extra gate's worth of objects for the same zero return.
    ///   `src/tests/perf/scenarios/09_alloc_ctorless` is this shape exactly — measured
    ///   **384 B and then 0 B** reclaimed against a 32 MB gate. Backing off is right, and
    ///   costs nothing: that heap is 100% live, so not collecting it does not even grow RSS
    ///   (measured 243 MB backed off vs 257 MB collecting anyway).
    /// - **Merely surviving**: a program holding on to most of what it allocates reclaims well
    ///   under half a nursery *while working perfectly*. `z42c.semantics` is this shape —
    ///   measured **8.4 MB and 10.1 MB** against the same 32 MB gate, i.e. 26% and 32% of it.
    ///   The half-gate bar read those as futile, took the multiplier to 4, and the two minors
    ///   that followed scanned 96 MB and 160 MB of nursery and paused **45.4 ms / 64.6 ms** —
    ///   39% of the whole build's pause, where every honest-gate minor cost 6–22 ms.
    ///
    /// The second case is the one that hurts, and it hurts **backwards**: the multiplier lands
    /// on the nursery, which is not a memory gate but the bound on how much young set one minor
    /// has to chew through — so the answer to "this program has a high survival rate" came out
    /// as "then scan four times as much next time". `minor_escalation_threshold` is the
    /// mechanism that already handles "minors are not helping": it escalates to a **major**,
    /// which is the collection that can actually do something about an old generation.
    ///
    /// [`FUTILE_DIVISOR`] separates them with room to spare: 1/16 of a gate is far below
    /// anything a working collection returns and far above the handful of bytes a
    /// 100%-survival workload gives back — 2 MB against the 384 B / 8.4 MB pair above.
    ///
    /// `cycles == 0` means no collection has happened yet, so there is nothing to judge:
    /// reading its absent 0 reclaimed as futile penalised a heap that had never been collected.
    fn next_backoff(backoff: u32, reclaimed_since: u64, gate: u64, cycles: u64) -> u32 {
        if cycles == 0 {
            1
        } else if reclaimed_since < gate / FUTILE_DIVISOR {
            // Freed essentially nothing — climb fast, because each successive futile collection
            // is *more* expensive than the last (a whole extra gate to mark for the same zero).
            backoff.saturating_mul(4).min(MAX_BACKOFF)
        } else {
            1
        }
    }

    fn decide_trip(
        &self,
        used: u64,
        baseline: u64,
        backoff: u32,
        soft_limit: Option<u64>,
        cfg: &crate::config::RuntimeConfig,
    ) -> Option<Trip> {
        let nursery = self.nursery_bytes();
        let allowance = Self::collection_allowance(baseline, nursery, soft_limit);
        let grown = used.saturating_sub(baseline);
        // A heap already past its soft cap is under real pressure: collect regardless of how
        // little it has grown since last time (the backoff still keeps this from spinning).
        let near_cap = soft_limit.is_some_and(|l| used >= (l as f64 * cfg.gc_near_limit_ratio) as u64);

        if crate::gc::MagrGC::mode(self) != crate::gc::GcMode::GenerationalMarkSweep {
            // One generation: every collection is a full one, and the allowance is read off
            // total live bytes rather than promoted bytes.
            let gate = allowance.saturating_mul(backoff as u64);
            return (grown >= gate || near_cap).then_some(Trip { major: true, gate: allowance });
        }

        // Old generation first — a minor cannot sweep it at all, so if it is full, that is
        // the collection we need.
        let promoted = self.promoted_bytes_since_major.load(Ordering::Relaxed);
        if promoted >= allowance || near_cap {
            return Some(Trip { major: true, gate: allowance });
        }
        let minor_gate = self.minor_gate(baseline, soft_limit);
        let gate = minor_gate.saturating_mul(backoff as u64);
        (grown >= gate).then_some(Trip { major: false, gate: minor_gate })
    }

    /// Mono SGen's allowance rule (`sgen_memgov_calculate_minor_collection_allowance`):
    /// let the old generation take in a third of what the last full collection left alive
    /// before sweeping it again, but never less than a few nurseries' worth — otherwise a
    /// program with a tiny live set would collect constantly.
    ///
    /// A soft cap squeezes it from the other side: once `live + allowance` would cross the
    /// cap, the allowance shrinks to whatever headroom is left (floored at the minimum, so a
    /// heap whose live set already exceeds its cap degrades to "collect every minimum" rather
    /// than "collect on every allocation").
    fn collection_allowance(live: u64, nursery: u64, soft_limit: Option<u64>) -> u64 {
        let mut min_allowance = nursery.saturating_mul(ALLOWANCE_NURSERY_RATIO);
        if let Some(cap) = soft_limit {
            min_allowance = min_allowance.min(((cap as f64) * ALLOWANCE_HEAP_RATIO) as u64);
        }
        let min_allowance = min_allowance.max(1);
        let mut allowance = (((live as f64) * ALLOWANCE_HEAP_RATIO) as u64).max(min_allowance);
        if let Some(cap) = soft_limit {
            if live.saturating_add(allowance) > cap {
                allowance = cap.saturating_sub(live).max(min_allowance);
            }
        }
        allowance
    }

    /// **flip-gc-default-to-generational (2026-09-10)**: how much may be allocated before a
    /// **minor** — the nursery, but never more than the whole collection allowance.
    ///
    /// With no soft cap the allowance is at least `ALLOWANCE_NURSERY_RATIO` (4) nurseries, so
    /// this is exactly `nursery` and nothing changes — that is the default path. A soft cap
    /// squeezes the allowance from the other side (see [`Self::collection_allowance`]), and
    /// once it squeezes below one nursery the **cap** is what should decide when to collect:
    /// a minor still only scans the young set, so running it more often costs pause time
    /// proportional to the young set, not to the budget.
    ///
    /// Without this the nursery — an absolute 32 MB by default, deliberately independent of
    /// any budget — was the only generational gate, so a small `Z42_GC_MAX_BYTES` was not
    /// enforced until the heap had allocated a whole nursery past it. Invisible while STW was
    /// the default; the default path now.
    fn minor_gate(&self, baseline: u64, soft_limit: Option<u64>) -> u64 {
        let nursery = self.nursery_bytes();
        nursery.min(Self::collection_allowance(baseline, nursery, soft_limit))
    }

    /// Set the next `used_bytes` reading at which the policy wants to be consulted.
    ///
    /// `floor` keeps it strictly ahead of the current reading even when the heap has already
    /// blown past the gate (a declined trip must not re-enter on the very next allocation).
    fn arm_next_collect(
        &self,
        baseline: u64,
        floor: u64,
        backoff: u32,
        soft_limit: Option<u64>,
    ) {
        let gate = if crate::gc::MagrGC::mode(self) == crate::gc::GcMode::GenerationalMarkSweep {
            self.minor_gate(baseline, soft_limit)
        } else {
            Self::collection_allowance(baseline, self.nursery_bytes(), soft_limit)
        };
        let gate = gate.saturating_mul(backoff.max(1) as u64);
        let next = baseline.saturating_add(gate).max(floor.saturating_add(gate));
        self.next_collect_at.store(next, Ordering::Relaxed);
    }

    /// **arm-gc-by-default (2026-09-09)**: recompute the trip point from the live set a
    /// collection just swept down to. Called from `sub_used_bytes`, the one point every
    /// collect path passes through — and therefore **lock-free**, because three of those four
    /// call sites are already holding `inner.lock()`.
    ///
    /// The backoff multiplier lives behind that mutex, so it is not applied here; the slow
    /// path re-arms with it the next time it runs, which is at worst one gate later.
    pub(super) fn rearm_auto_collect(&self) {
        let live = self.used_bytes_atomic();
        let nursery = self.nursery_bytes();
        let soft_limit = match self.max_bytes_atomic.load(Ordering::Relaxed) {
            u64::MAX => None,
            n => Some(n),
        };
        let allowance = Self::collection_allowance(live, nursery, soft_limit);
        if crate::gc::MagrGC::mode(self) != crate::gc::GcMode::GenerationalMarkSweep {
            self.next_collect_at
                .store(live.saturating_add(allowance), Ordering::Relaxed);
            return;
        }
        // The old generation may already be over its allowance — a minor cannot help with
        // that, so ask to be consulted immediately and let `decide_trip` call for a major.
        let next = if self.promoted_bytes_since_major.load(Ordering::Relaxed) >= allowance {
            live
        } else {
            // Same gate as `decide_trip` / `arm_next_collect` — see [`Self::minor_gate`]. This
            // is the arming point every *collect* path lands on, so a plain `nursery` here left
            // a bounded heap un-enforced from its very first allocation.
            live.saturating_add(self.minor_gate(live, soft_limit))
        };
        self.next_collect_at.store(next, Ordering::Relaxed);
    }

    /// **arm-gc-by-default (2026-09-09)**: how many bytes may be allocated before a **minor**
    /// trips. `Z42_GC_NURSERY_BYTES` when set, otherwise [`DEFAULT_NURSERY_BYTES`].
    ///
    /// An **absolute** default, not a fraction of the budget it used to be derived from: the
    /// nursery bounds *minor work*, which is unrelated to how large the heap may grow, and a
    /// derived default would have made the whole policy depend on a budget existing.
    /// Never zero — a zero nursery would trip a collection on every allocation.
    #[inline]
    fn nursery_bytes(&self) -> u64 {
        self.nursery_bytes.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "auto_collect_tests.rs"]
mod auto_collect_tests;
