//! **Incremental major collection** — add-incremental-major-gc M2b (2026-09-16).
//!
//! A generational major no longer runs in one pause. It runs as a sequence of **slices**, each an
//! ordinary stop-the-world pause bounded by `Z42_GC_SLICE_MS`, with mutators — and minors — running
//! in between:
//!
//! ```text
//!  Idle ─trip→ [open: epoch++, SATB on, alloc-black on, grey the roots] ─→ Marking
//!  Marking:   trace the grey queue within budget; queue empty → grey the SATB records → repeat;
//!             both empty → revive soft refs once → SATB off ─→ Sweeping
//!  Sweeping:  objects → arrays → var blocks, one chunk at a time within budget
//!  wrap-up:   chunk reclaim, alloc-black off, contexts / soft registry, promotion bookkeeping ─→ Idle
//! ```
//!
//! Marking never runs **concurrently** with a mutator — between two slices the world simply moves
//! on, and three invariants carry the argument across the gap:
//!
//! 1. **SATB** (`gc::satb`): an edge overwritten while marking hands its old target to the marker,
//!    so everything reachable at the root snapshot gets marked (Yuasa).
//! 2. **Allocate-black** (`alloc_black.rs`): everything born during the cycle carries its epoch, so
//!    neither the marker nor the sweep has to know it exists.
//! 3. **A minor treats the grey queue and SATB records as roots** (`mark_phase_minor`), so it never
//!    reclaims something the marker still has to visit.
//!
//! ## What *Sweeping* adds: doomed entries
//!
//! Once marking is complete, every alive entry either carries the cycle's epoch or is garbage the
//! sweep has not reached yet — **doomed**. A doomed entry is still `alive`, and its children may
//! already have been reclaimed by an earlier sweep slice (the cursor is not in graph order), so it
//! must never be traced or handed to a mutator again:
//!
//! - weak / soft reads and heap iteration refuse it ([`ArcMagrGC::admit_resurrected`]);
//! - a minor's dirty-card seeding skips it (`seed_from_dirty_cards`) — a dead old object in a dirty
//!   card is otherwise traced *through*, straight into a slot that may hold a new object by now;
//! - the debug validator does not look at epochs or the grey queue while a cycle is open.
//!
//! The fixed regions' sweep also has to delist the dead from `young_list` as it goes
//! (`Region::sweep_chunks(delist_young = true)`) — see there.
//!
//! ## Scheduling
//!
//! The auto-collect policy (`auto_collect.rs`) opens a cycle where it used to run a major, and
//! while one is open asks for a slice every [`ArcMagrGC::slice_interval`] bytes of heap growth.
//! Explicit collections (`GC.Collect()`, `force_collect`) and a heap at its soft cap **finish the
//! cycle synchronously** — correctness first, the pause bound is given up for that one call.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

use parking_lot::Mutex;

use crate::gc::phase_timer::PhaseTimer;
use crate::gc::refs::MarkKind;
use crate::metadata::Value;

const IDLE: u8 = 0;
const MARKING: u8 = 1;
const SWEEPING: u8 = 2;

/// How often (in work units — traced values, swept entries) a native budget reads the clock.
const CLOCK_EVERY: u32 = 64;

/// wasm32 has no clock (`std::time` panics there), so a slice is measured in work units instead.
#[cfg(target_arch = "wasm32")]
const UNITS_PER_MS: f64 = 10_000.0;

/// The per-heap state of the incremental major. One field on `ArcMagrGC`, so the heap's own
/// struct stays small.
#[derive(Default)]
pub(crate) struct IncrementalState {
    /// [`IDLE`] / [`MARKING`] / [`SWEEPING`]. Changed only inside a slice (world stopped); read
    /// lock-free by weak reads, card seeding and the auto-collect policy.
    phase: AtomicU8,
    /// Cursor and bookkeeping; only a slice touches it.
    cycle: Mutex<Cycle>,
    /// Which collection the auto-collect policy asked for. The deferred safepoint path only knows
    /// "collect", so the kind travels in these; a collection that finds none of them set was not
    /// asked for by the policy — it is an explicit `GC.Collect()`.
    pub(super) pending_minor: AtomicBool,
    pub(super) pending_slice: AtomicBool,
    /// The heap reached its soft cap while a cycle was open: finish it now.
    pub(super) pending_finish: AtomicBool,
    /// A cycle deferred behind a minor (see `choose_generational_work`): open it next.
    pub(super) pending_open: AtomicBool,
    /// The `used_bytes` reading at which the open cycle wants its next slice.
    pub(super) next_slice_at: AtomicU64,
    /// Heap growth between two slices, as last set by the pacer (0 = none yet).
    pub(super) slice_interval: AtomicU64,
    /// Work units the last completed cycle took — the next cycle's estimate.
    last_cycle_units: AtomicU64,
}

#[derive(Default)]
struct Cycle {
    soft_revived: bool,
    live_contexts: Option<std::collections::HashSet<crate::metadata::context::ContextId>>,
    stage: SweepStage,
    cursor: usize,
    slices: u32,
    work_us: u64,
    max_slice_us: u64,
    traced: usize,
    reclaimed: usize,
    /// Work units spent sweeping (`CHUNK_SIZE` per object / array chunk, `CLOCK_EVERY` per bucket).
    sweep_units: u64,
    /// Pacer inputs: `used_bytes` when the cycle opened, the growth it should finish within, and
    /// the work it is expected to take.
    start_used: u64,
    target_growth: u64,
    est_units: u64,
}

impl Cycle {
    fn units(&self) -> u64 {
        self.traced as u64 + self.sweep_units
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum SweepStage {
    #[default]
    Objects,
    Arrays,
    Var,
    Done,
}

/// What one generational pause does (see [`ArcMagrGC::choose_generational_work`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GenWork {
    Minor,
    /// One-shot major (`Z42_GC_INCREMENTAL=0`).
    Major,
    /// Open a cycle, or advance the open one by one slice.
    Slice,
    /// Run the open cycle to completion in this pause.
    Finish,
}

/// What a slice (or a synchronous finish) did.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct SliceOutcome {
    pub(super) freed_bytes: u64,
    /// The cycle completed in this pause.
    pub(super) finished: bool,
}

/// The budget of one slice: a deadline on native, a work-unit count on wasm32 (no clock there) and
/// in tests (deterministic interleavings).
pub(super) struct SliceBudget {
    #[cfg(not(target_arch = "wasm32"))]
    deadline: Option<std::time::Instant>,
    units_left: Option<u64>,
    #[cfg(not(target_arch = "wasm32"))]
    since_check: u32,
    spent: bool,
}

impl SliceBudget {
    /// No limit: a synchronous finish.
    pub(super) fn unlimited() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            deadline: None,
            units_left: None,
            #[cfg(not(target_arch = "wasm32"))]
            since_check: 0,
            spent: false,
        }
    }

    /// `Z42_GC_SLICE_MS`.
    pub(super) fn from_config() -> Self {
        let ms = crate::config::runtime_config().gc_slice_ms;
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                deadline: Some(std::time::Instant::now() + std::time::Duration::from_secs_f64(ms / 1000.0)),
                ..Self::unlimited()
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::from_units((ms * UNITS_PER_MS).max(1.0) as u64)
        }
    }

    /// A budget of `units` work units (traced values; `CHUNK_SIZE` per swept chunk).
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(super) fn from_units(units: u64) -> Self {
        Self { units_left: Some(units.max(1)), ..Self::unlimited() }
    }

    /// Charge `units` of work. Returns `true` once the budget is used up (and keeps returning it).
    #[inline]
    fn spend(&mut self, units: u32) -> bool {
        if self.spent {
            return true;
        }
        if let Some(left) = self.units_left.as_mut() {
            *left = left.saturating_sub(units as u64);
            self.spent = *left == 0;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(deadline) = self.deadline {
            self.since_check += units;
            if self.since_check >= CLOCK_EVERY {
                self.since_check = 0;
                self.spent |= std::time::Instant::now() >= deadline;
            }
        }
        self.spent
    }
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// Whether an incremental major cycle is open (marking or sweeping).
    #[inline]
    pub(crate) fn major_cycle_active(&self) -> bool {
        self.incremental.phase.load(Ordering::Relaxed) != IDLE
    }

    /// Whether the open cycle has finished marking and is sweeping.
    #[inline]
    fn major_cycle_sweeping(&self) -> bool {
        self.incremental.phase.load(Ordering::Relaxed) == SWEEPING
    }

    /// The epoch of the open cycle **if it is sweeping**: an alive entry without it is doomed.
    #[inline]
    pub(super) fn doomed_unless_marked(&self) -> Option<MarkKind> {
        self.major_cycle_sweeping().then(|| self.major_mark())
    }

    /// Gate for every path that hands an existing heap value to a mutator **without** going
    /// through a strong reference: weak / soft reads and heap iteration.
    ///
    /// - While a mark is in progress the value is shaded (it may have been only weakly reachable
    ///   at the snapshot, and it is about to be in a register) — `shade_if_marking`.
    /// - While an incremental cycle is sweeping, a doomed value is refused: returning it would put
    ///   a handle the sweep is about to reclaim — and whose children may be gone already — into a
    ///   register. Returns `false` for exactly that case.
    pub(crate) fn admit_resurrected(&self, v: &Value) -> bool {
        if let Some(kind) = self.doomed_unless_marked() {
            return Self::is_marked_value(v, kind);
        }
        self.shade_if_marking(v);
        true
    }

    /// Decide what this generational pause does, consuming the policy's request flags.
    /// `want_major` is the promoted-byte gate, escalation, or adaptive promotion asking for a major.
    pub(super) fn choose_generational_work(&self, want_major: bool) -> GenWork {
        let s = &self.incremental;
        let want_minor = s.pending_minor.swap(false, Ordering::AcqRel);
        let want_slice = s.pending_slice.swap(false, Ordering::AcqRel);
        let want_finish = s.pending_finish.swap(false, Ordering::AcqRel);
        let want_open = s.pending_open.swap(false, Ordering::AcqRel);
        if !crate::config::runtime_config().gc_incremental && !self.major_cycle_active() {
            return if want_major { GenWork::Major } else { GenWork::Minor };
        }
        if !self.major_cycle_active() {
            if want_major && want_minor {
                // Never let the cycle displace a minor the policy asked for: minors are what age
                // the young generation, and an incremental cycle has no aging pass of its own
                // (the one-shot major needs one for exactly this reason). Run the minor; the cycle
                // opens at the next safepoint.
                s.pending_open.store(true, Ordering::Release);
                return GenWork::Minor;
            }
            // A slice request with no cycle open is stale (something finished the cycle after the
            // policy asked) — it must not open a new one.
            return if want_major || want_open { GenWork::Slice } else { GenWork::Minor };
        }
        if want_finish || !(want_minor || want_slice || want_major) {
            // Nothing the policy asked for: an explicit `GC.Collect()`, which promises that
            // everything unreachable is gone when it returns.
            return GenWork::Finish;
        }
        if want_minor { GenWork::Minor } else { GenWork::Slice }
    }

    /// One slice of the incremental major — opens a cycle if none is open. The caller holds the
    /// pause.
    pub(super) fn run_major_slice(&self, budget: &mut SliceBudget) -> SliceOutcome {
        let t0 = Self::now_us();
        // Merge this thread's TLAB (and SATB records); every other mutator did at its park.
        self.retire_thread_tlab();
        let mut cycle = self.incremental.cycle.lock();
        if !self.major_cycle_active() {
            self.open_incremental_cycle(&mut cycle);
        }
        if self.incremental.phase.load(Ordering::Relaxed) == MARKING {
            let t = PhaseTimer::start("slice/mark");
            let before = cycle.traced;
            if self.mark_slice(&mut cycle, budget) {
                self.finish_incremental_marking(&mut cycle);
            }
            t.count(cycle.traced - before);
        }
        let mut out = SliceOutcome::default();
        if self.major_cycle_sweeping() {
            let t = PhaseTimer::start("slice/sweep");
            let before = cycle.reclaimed;
            let (freed, done) = self.sweep_slice(&mut cycle, budget);
            t.count(cycle.reclaimed - before);
            out.freed_bytes = freed;
            if done {
                drop(t);
                self.close_incremental_cycle(&mut cycle);
                out.finished = true;
            }
        }
        let us = Self::now_us().saturating_sub(t0);
        cycle.slices += 1;
        cycle.work_us += us;
        cycle.max_slice_us = cycle.max_slice_us.max(us);
        if out.finished {
            crate::gc::phase_timer::note(format_args!(
                "major cycle {:?}: slices {}  work {:.2} ms  max slice {:.2} ms  traced {}  reclaimed {}",
                self.major_mark(), cycle.slices, cycle.work_us as f64 / 1000.0,
                cycle.max_slice_us as f64 / 1000.0, cycle.traced, cycle.reclaimed));
            self.incremental.last_cycle_units.store(cycle.units(), Ordering::Relaxed);
            *cycle = Cycle::default();
        } else {
            self.pace(&cycle, self.used_bytes_atomic().saturating_sub(out.freed_bytes));
        }
        out
    }

    /// **The pacer.** Space the remaining slices so the cycle completes before the heap has grown
    /// `target_growth` past where it opened: remaining work (estimate minus done) over the work one
    /// slice does gives the slices left; the bytes left before the target, divided by that, is the
    /// growth to allow between two slices.
    ///
    /// A fixed interval cannot do this job. Measured on `13_gc_large_heap` with one slice per
    /// quarter nursery: 30~37 slices per cycle stretched a cycle over ~8 nurseries of allocation,
    /// everything the churn freed meanwhile floated to the next cycle, and peak RSS went from
    /// 878 MB to 1.42 GB.
    ///
    /// Behind schedule (past the target, or the estimate was low) the interval bottoms out at
    /// [`MIN_SLICE_INTERVAL`](super::auto_collect) — slices come almost back to back, each still
    /// bounded: throughput is what gives, never the pause bound.
    fn pace(&self, cycle: &Cycle, used: u64) {
        let done = cycle.units();
        let est = cycle.est_units.max(done + done / 8 + 1);
        let per_slice = (done / u64::from(cycle.slices.max(1))).max(1);
        let slices_left = ((est - done) / per_slice).max(1);
        let bytes_left = (cycle.start_used + cycle.target_growth).saturating_sub(used);
        let interval = self.clamp_slice_interval(bytes_left / slices_left);
        self.incremental.slice_interval.store(interval, Ordering::Relaxed);
        self.incremental.next_slice_at.store(used + interval, Ordering::Relaxed);
    }

    /// Run the open cycle to completion in the current pause. No-op (zero) when none is open.
    pub(super) fn finish_major_cycle(&self, reason: &str) -> SliceOutcome {
        if !self.major_cycle_active() {
            return SliceOutcome::default();
        }
        crate::gc::phase_timer::note(format_args!("incremental: finish synchronously ({reason})"));
        let mut total = SliceOutcome::default();
        let mut budget = SliceBudget::unlimited();
        while !total.finished {
            let o = self.run_major_slice(&mut budget);
            total.freed_bytes += o.freed_bytes;
            total.finished = o.finished;
        }
        total
    }

    /// A minor just ran inside an open cycle: make sure the next slice is not pushed further out by
    /// what the minor reclaimed (slices are paced off `used_bytes`, which a minor lowers).
    pub(super) fn after_minor_in_cycle(&self) {
        if !self.major_cycle_active() {
            // A cycle deferred behind this minor (see `choose_generational_work`) opens at the
            // next safepoint.
            if self.incremental.pending_open.load(Ordering::Acquire) {
                if let Some(flag) = self.external_needs_collect.lock().as_ref() {
                    flag.store(true, Ordering::Release);
                }
            }
            return;
        }
        let next = self.used_bytes_atomic() + self.slice_interval();
        self.incremental.next_slice_at.fetch_min(next, Ordering::Relaxed);
        self.rearm_auto_collect();
    }

    fn open_incremental_cycle(&self, cycle: &mut Cycle) {
        let used = self.used_bytes_atomic();
        *cycle = Cycle {
            start_used: used,
            target_growth: self.cycle_target_growth(used),
            est_units: self.estimate_cycle_units(),
            ..Cycle::default()
        };
        self.open_major_cycle();
        self.begin_alloc_black();
        let roots = self.snapshot_roots_into_mark_queue();
        self.incremental.phase.store(MARKING, Ordering::Relaxed);
        crate::gc::phase_timer::note(format_args!(
            "incremental: open cycle {:?}  roots {roots}  est {} units  target +{}",
            self.major_mark(), cycle.est_units, crate::gc::trace::human(cycle.target_growth)));
    }

    /// Work units the cycle about to open should take: the last cycle's, or — for the first — an
    /// upper bound read off the regions in O(1) (every fixed slot traced and swept once, every
    /// live var block traced, every var bucket swept).
    fn estimate_cycle_units(&self) -> u64 {
        match self.incremental.last_cycle_units.load(Ordering::Relaxed) {
            0 => {
                let slots = (self.region_object.lock().chunk_count() + self.region_array.lock().chunk_count())
                    * crate::gc::region::CHUNK_SIZE;
                let var = self.region_var.lock();
                (2 * slots + var.live_count() + var.bucket_count() * CLOCK_EVERY as usize) as u64
            }
            n => n,
        }
    }

    /// Trace grey values until the budget runs out or the grey set, the SATB records and the soft
    /// revive all come up empty. Returns whether marking is complete.
    fn mark_slice(&self, cycle: &mut Cycle, budget: &mut SliceBudget) -> bool {
        let kind = self.major_mark();
        let mut stack: Vec<Value> = std::mem::take(&mut *self.mark_queue.lock());
        loop {
            while let Some(v) = stack.pop() {
                cycle.traced += 1;
                v.trace_children(kind, &mut |child| {
                    if Self::mark_if_unmarked(child, kind) {
                        stack.push(child.clone());
                    }
                });
                if budget.spend(1) {
                    // Hand the rest back: the queue is a minor's root set between slices.
                    self.mark_queue.lock().append(&mut stack);
                    return false;
                }
            }
            for v in std::mem::take(&mut *self.satb_queue.lock()) {
                if Self::mark_if_unmarked(&v, kind) {
                    stack.push(v);
                }
            }
            if !stack.is_empty() {
                continue;
            }
            if !cycle.soft_revived {
                // Soft targets the pressure policy keeps: marked here, and — unlike the one-shot
                // path — queued, so what they reference is kept too.
                cycle.soft_revived = true;
                self.revive_soft_refs_into(&mut stack, kind);
                if !stack.is_empty() {
                    continue;
                }
            }
            return true;
        }
    }

    fn revive_soft_refs_into(&self, stack: &mut Vec<Value>, kind: MarkKind) {
        let used = self.used_bytes_atomic();
        let (entries, max_bytes) = {
            let inner = self.inner.lock();
            (inner.soft_registry.snapshot_entries(), inner.stats.max_bytes.unwrap_or(0))
        };
        crate::gc::soft_registry::SoftRegistry::revive_snapshot(&entries, used, max_bytes, kind, |e| {
            stack.push(Self::soft_entry_value(e));
        });
    }

    fn finish_incremental_marking(&self, cycle: &mut Cycle) {
        crate::gc::satb::end_marking(self.epoch);
        // add-lazy-context-unload: which unloading contexts marked objects retain — read now,
        // while the marks are complete and nothing has been swept.
        let snapshot = self.context_reclaimer.lock().as_ref()
            .filter(|r| r.is_unloading())
            .map(|r| r.snapshot());
        cycle.live_contexts = snapshot.as_ref().map(|s| self.scan_marked_contexts(s));
        cycle.stage = SweepStage::Objects;
        cycle.cursor = 0;
        self.incremental.phase.store(SWEEPING, Ordering::Relaxed);
    }

    /// Sweep chunk by chunk until the budget runs out. Returns `(freed bytes, sweep complete)`.
    fn sweep_slice(&self, cycle: &mut Cycle, budget: &mut SliceBudget) -> (u64, bool) {
        let kind = self.major_mark();
        let mut freed = 0u64;
        while cycle.stage != SweepStage::Done {
            let (units, done) = match cycle.stage {
                SweepStage::Objects => {
                    let mut region = self.region_object.lock();
                    let (f, r, next) = region.sweep_chunks(kind, cycle.cursor, 1, true, Self::prepare_dead_object);
                    freed += f;
                    cycle.reclaimed += r;
                    cycle.cursor = next;
                    (crate::gc::region::CHUNK_SIZE as u32, next >= region.chunk_count())
                }
                SweepStage::Arrays => {
                    let mut region = self.region_array.lock();
                    let (f, r, next) = region.sweep_chunks(kind, cycle.cursor, 1, true, Self::prepare_dead_array);
                    freed += f;
                    cycle.reclaimed += r;
                    cycle.cursor = next;
                    (crate::gc::region::CHUNK_SIZE as u32, next >= region.chunk_count())
                }
                SweepStage::Var => {
                    let mut region = self.region_var.lock();
                    let (r, credited, next) = region.sweep_buckets(kind, cycle.cursor, 1);
                    freed += credited;
                    cycle.reclaimed += r;
                    cycle.cursor = next;
                    (CLOCK_EVERY, next >= region.bucket_count())
                }
                SweepStage::Done => unreachable!(),
            };
            cycle.sweep_units += u64::from(units);
            if done {
                cycle.cursor = 0;
                cycle.stage = match cycle.stage {
                    SweepStage::Objects => SweepStage::Arrays,
                    SweepStage::Arrays => SweepStage::Var,
                    _ => SweepStage::Done,
                };
            }
            if cycle.stage != SweepStage::Done && budget.spend(units) {
                return (freed, false);
            }
        }
        (freed, true)
    }

    /// Everything the one-shot major does after its sweep, in the slice that completes the sweep.
    fn close_incremental_cycle(&self, cycle: &mut Cycle) {
        {
            let _t = PhaseTimer::start("slice/chunk reclaim");
            self.region_object.lock().reclaim_dead_chunks();
            self.region_array.lock().reclaim_dead_chunks();
            self.region_var.lock().reclaim_dead_var_chunks();
        }
        // Sweep has cleared no marks (the epoch whitens them next cycle), so a newborn of this
        // cycle simply leaves it carrying an epoch the next one has moved past.
        self.end_alloc_black();
        if let Some(live) = cycle.live_contexts.take() {
            if let Some(r) = self.context_reclaimer.lock().as_ref() {
                r.reclaim(&live);
            }
        }
        self.inner.lock().soft_registry.prune_dead();
        // No aging pass: minors kept running between the slices, and aging is theirs. The one
        // exception is a pending promotion-age switch, which needs a major under it.
        self.apply_adaptive_promotion_after_major(true);
        self.promoted_bytes_since_major.store(0, Ordering::Relaxed);
        self.incremental.phase.store(IDLE, Ordering::Relaxed);
    }
}

/// A heap dropped in the middle of a mark must stop the process-wide SATB barrier recording for
/// it — otherwise every other heap's writes stay on the barrier's slow path for good.
impl Drop for crate::gc::arc_heap::ArcMagrGC {
    fn drop(&mut self) {
        if self.incremental.phase.load(Ordering::Relaxed) == MARKING {
            crate::gc::satb::end_marking(self.epoch);
        }
    }
}

#[cfg(test)]
impl crate::gc::arc_heap::ArcMagrGC {
    /// Drive one slice by hand with a budget of `units` work units (tests are single-threaded, so
    /// there is no pause to take). Returns whether the cycle completed.
    pub(crate) fn run_major_slice_for_test(&self, units: u64) -> bool {
        self.run_major_slice(&mut SliceBudget::from_units(units)).finished
    }

    /// Finish the open cycle (if any), as `GC.Collect()` would.
    pub(crate) fn finish_major_cycle_for_test(&self) -> bool {
        self.finish_major_cycle("test").finished
    }

    /// Whether the open cycle is still marking / already sweeping.
    pub(crate) fn major_cycle_marking_for_test(&self) -> bool {
        self.incremental.phase.load(Ordering::Relaxed) == MARKING
    }

    pub(crate) fn major_cycle_sweeping_for_test(&self) -> bool {
        self.major_cycle_sweeping()
    }
}
