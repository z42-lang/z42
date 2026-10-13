//! **Young-generation policy** (M9): what a minor trip does once minors stop paying for
//! themselves.
//!
//! # The problem it solves
//!
//! A minor costs the young set it marks plus a fixed part (the card scan is O(old generation)),
//! and it is worth that only while the young set is mostly garbage. Two shapes break that:
//!
//! - **Nothing dies** (`09_alloc_ctorless`, a data structure being built): every minor is pure
//!   cost, and re-marking the same survivors at every gate until they reach the promotion age
//!   is the worst of it.
//! - **Little dies, but some does** (`13_gc_large_heap` while a major cycle is open: ~90% of
//!   each nursery survives): a minor frees ~0.5 MB for ~13 ms, while the major cycle running
//!   alongside frees ~0.7 MB **per millisecond** of its slices.
//!
//! Growing the minor gate (a futility multiplier, ×4 per futile minor) answers the first shape by
//! collecting less often while the young set grows — and a later phase with garbage then pays for
//! it in one minor over the whole backed-off young set (`13_gc_large_heap` 67~144 ms, binary-trees
//! 101~137 ms). It cannot see the second shape at all: 0.5 MB of a 4 MB gate is above a 1/16 bar.
//!
//! # The rule
//!
//! ```text
//!  after every real minor:  unproductive = freed < gate / FUTILE_DIVISOR                      (Futile, bytes)
//!                                        || reclaimed entries < scanned / FUTILE_DIVISOR      (Futile, entries)
//!                                        || survivors ≥ pause budget in entries               (OverBudget)
//!                                        || freed / pause  <  (major freed / major pause) / MAJOR_YIELD_MARGIN
//!                           unproductive → run = clamp(2 × run, 1, MAX_TENURE_RUN); the next `run` trips tenure
//!                           productive   → run = 0
//!  at a minor trip:         a tenure turn left → **tenure** (promote the young set, no mark)
//!                           else              → a real minor (which is also the probe)
//! ```
//!
//! **Entries as well as bytes.** The futility bar is read both ways — bytes freed against the
//! gate, entries reclaimed against entries scanned — so that the same program makes the same
//! call whatever its objects weigh: an object model that packs a field-less object into 16 B
//! instead of 32 B halves every byte figure, and a byte-only rule then shifts where in the
//! program's phases each verdict lands.
//!
//! **Over budget.** The pause budget (`pause_budget.rs`) knows what one young entry costs a minor,
//! how many entries the pause target buys, and how many fresh entries the nursery it just chose
//! will add. When the survivors this minor leaves listed reach what is left of the budget after
//! those — the model already moved the nursery as far as it may (at most halving it, never under
//! the floor) — the next minor busts the target re-marking what this one just marked. The nursery
//! cannot help further (it bounds only the fresh entries), and those survivors stay listed until
//! they have aged `promotion_age` minors; they also have to be at least half of what this minor
//! scanned, so a minor whose per-entry cost is inflated by its fixed part keeps running. That is the same "minors stop
//! paying" situation as the two shapes above, judged by measured cost rather than by yield, and it
//! gets the same answer: tenure. Measured shape (`13_gc_large_heap` with a denser object layout):
//! a minor reclaimed 5.2 MB (productive by bytes and by entries) but left 675 k survivors while
//! the budget (714 k entries at 14 ns) minus the 4.5 MB floor nursery's 87 k fresh entries left
//! them 627 k, and the next minor re-scanned 765 k entries for 16–19 ms where the byte-only rule's
//! tenure had kept it at 12 ms.
//!
//! **Yield per unit of pause, against the major's.** A byte of young garbage can be reclaimed
//! by a minor now, or tenured and reclaimed by the next major. Tenuring adds it to the old
//! generation's growth, which is what triggers majors, so its price is the major's cost per byte
//! reclaimed — `1 / major yield`. The minor's price is `1 / minor yield`. Comparing the two in the
//! same unit (bytes per microsecond of pause, on the same machine, in the same run) needs no
//! absolute threshold. `MAJOR_YIELD_MARGIN` keeps the comparison one-sided: tenured garbage also
//! costs footprint until that major, and a minor within a factor of 4 of the major keeps running.
//! Measured: `13_gc_large_heap` minors in a cycle 0.02~0.06 MB/ms vs its majors 0.6~1.2 (20×);
//! `z42c` minors 0.5~1.4 vs majors 1.1~1.5 (≤ 2.5×) — the margin falls between them.
//!
//! **Tenure, not a bigger gate.** A tenure walks the young lists once, sets every entry's age to
//! the promotion age and empties the lists: no tracing, no card scan, no sweep — about a tenth of
//! the minor it replaces, and it does not grow with how long the phase lasts. The gate stays the
//! nursery the pause budget chose, so the young set never exceeds one nursery and the next real
//! minor costs what an ordinary one does.
//!
//! **Probing.** A tenured nursery cannot be judged (nothing was marked), so every `run` tenures
//! a real minor re-measures; `run` doubles while minors stay unproductive and resets on the first
//! productive one. `MAX_TENURE_RUN` bounds how much young garbage a phase change can push into
//! the old generation before the policy notices: that many nurseries.
//!
//! **The old generation's side.** Tenuring sends everything a no-garbage phase allocates to the
//! old generation, so such a heap reaches the promoted-byte gate. Two rules keep the major from
//! repeating the minor's waste: a major that freed essentially nothing stops minors' high survival
//! from escalating to another one, and while the last major **and** the last real minor were both
//! futile the promoted-byte gate waits one allowance floor longer (`auto_collect::promoted_gate`).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// A minor that reclaimed less than `gate / FUTILE_DIVISOR` freed essentially nothing — the
/// 100%-survival shape, recognisable before any major has been measured. The same bar the
/// one-generation backoff uses (see there for why 1/16).
use super::auto_collect::FUTILE_DIVISOR;

/// The futility bar in entries: a minor that reclaimed fewer than `scanned / this` of the entries
/// it scanned freed essentially nothing — the entry twin of `FUTILE_DIVISOR`.
const FUTILE_DIVISOR_ENTRIES: u32 = 16;

/// A minor is outyielded when its bytes-per-pause is below the last major's divided by this.
pub(super) const MAJOR_YIELD_MARGIN: u64 = 4;

/// Longest run of tenures between two probing minors.
pub(super) const MAX_TENURE_RUN: u32 = 8;

/// What one real minor reported.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MinorYield {
    pub(crate) freed: u64,
    pub(crate) pause_us: u64,
    /// The minor gate in force — the byte futility bar is read off it.
    pub(crate) gate: u64,
    /// Young entries the minor scanned, the ones it reclaimed, and the ones it left listed.
    pub(crate) scanned: u64,
    pub(crate) reclaimed: u64,
    pub(crate) survivors: u64,
    /// The part of the next minor's pause budget, in entries, left for the survivors this one
    /// leaves listed (`PauseBudget::survivor_budget`), if measured.
    pub(crate) survivor_budget: Option<u64>,
}

/// Why the policy judged a minor the way it did (diagnostics and tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Productive,
    /// Freed essentially nothing (by bytes or by entries).
    Futile,
    /// Its survivors alone fill the next minor's pause budget.
    OverBudget,
    /// Freed something, but far less per unit of pause than the last major.
    Outyielded,
}

/// The entry counts of one real minor, as the collector measured them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MinorEntries {
    pub(crate) scanned: u64,
    pub(crate) reclaimed: u64,
    pub(crate) survivors: u64,
}

#[derive(Debug)]
pub(crate) struct YoungPolicy {
    /// Pauses are microseconds (`false` on wasm32, where `now_us` counts ticks): the yield
    /// comparison is only made when they are.
    timed: bool,
    /// The last completed major: bytes it freed and the pause it took, summed over its slices.
    major_freed: AtomicU64,
    major_pause_us: AtomicU64,
    /// The open cycle's running totals, published into the two above when it completes.
    cycle_freed: AtomicU64,
    cycle_pause_us: AtomicU64,
    /// Tenures granted after the last unproductive minor, and how many are left.
    run: AtomicU32,
    left: AtomicU32,
    /// The last completed major freed less than its futility bar.
    major_futile: AtomicBool,
    /// The last young-generation collection was a tenure (it reclaims nothing by design).
    last_tenure: AtomicBool,
    /// The last real minor freed essentially nothing.
    minor_futile: AtomicBool,
}

impl YoungPolicy {
    pub(crate) fn new(timed: bool) -> Self {
        YoungPolicy {
            timed,
            major_freed: AtomicU64::new(0),
            major_pause_us: AtomicU64::new(0),
            cycle_freed: AtomicU64::new(0),
            cycle_pause_us: AtomicU64::new(0),
            run: AtomicU32::new(0),
            left: AtomicU32::new(0),
            major_futile: AtomicBool::new(false),
            last_tenure: AtomicBool::new(false),
            minor_futile: AtomicBool::new(false),
        }
    }

    /// Judge a real minor and schedule the tenures that follow it.
    pub(crate) fn observe_minor(&self, m: MinorYield) -> Verdict {
        let verdict = self.judge(m);
        self.minor_futile.store(verdict == Verdict::Futile, Ordering::Relaxed);
        let run = if verdict == Verdict::Productive {
            0
        } else {
            self.run.load(Ordering::Relaxed).saturating_mul(2).clamp(1, MAX_TENURE_RUN)
        };
        self.run.store(run, Ordering::Relaxed);
        self.left.store(run, Ordering::Relaxed);
        verdict
    }

    fn judge(&self, m: MinorYield) -> Verdict {
        if m.freed < m.gate / FUTILE_DIVISOR
            || (m.scanned > 0 && m.reclaimed < m.scanned / u64::from(FUTILE_DIVISOR_ENTRIES))
        {
            return Verdict::Futile;
        }
        // The inheritance must also be most of what this minor paid for: a minor whose cost per
        // entry is inflated by its fixed part (card scan, a cycle's grey queue) with few survivors
        // is not helped by tenuring them.
        if m.survivors.saturating_mul(2) >= m.scanned && m.survivor_budget.is_some_and(|b| m.survivors >= b) {
            return Verdict::OverBudget;
        }
        let (mf, mp) = (self.major_freed.load(Ordering::Relaxed), self.major_pause_us.load(Ordering::Relaxed));
        if self.timed && mf > 0 && mp > 0 && m.pause_us > 0 {
            // freed / pause < (mf / mp) / MARGIN, cross-multiplied.
            let lhs = u128::from(m.freed) * u128::from(mp) * u128::from(MAJOR_YIELD_MARGIN);
            if lhs < u128::from(mf) * u128::from(m.pause_us) {
                return Verdict::Outyielded;
            }
        }
        Verdict::Productive
    }

    /// One pause of major work (a slice, a synchronous finish, or a one-shot major). `finished`:
    /// the cycle completed in it, so its totals become the reference yield; `futile_below` is
    /// the bar under which the completed cycle counts as having freed nothing.
    pub(crate) fn observe_major_work(&self, freed: u64, pause_us: u64, finished: bool, futile_below: u64) {
        let f = self.cycle_freed.fetch_add(freed, Ordering::Relaxed) + freed;
        let p = self.cycle_pause_us.fetch_add(pause_us, Ordering::Relaxed) + pause_us;
        if finished {
            self.major_freed.store(f, Ordering::Relaxed);
            self.major_pause_us.store(p, Ordering::Relaxed);
            self.cycle_freed.store(0, Ordering::Relaxed);
            self.cycle_pause_us.store(0, Ordering::Relaxed);
            self.major_futile.store(f < futile_below, Ordering::Relaxed);
        }
    }

    /// The last completed major freed essentially nothing. A minor's high survival is then no
    /// evidence that garbage sits in the old generation — the last look found none — so it must
    /// not escalate to another major; the promoted-byte gate still bounds the old generation.
    pub(crate) fn major_was_futile(&self) -> bool {
        self.major_futile.load(Ordering::Relaxed)
    }

    /// **Nothing is dying anywhere**: the last major and the last real minor both freed
    /// essentially nothing. Only then is the next major worth postponing — a futile major followed
    /// by minors that do reclaim (a program past its start-up) says garbage is being made again,
    /// and some of it is reaching the old generation.
    pub(crate) fn nothing_dies(&self) -> bool {
        self.major_was_futile() && self.minor_futile.load(Ordering::Relaxed)
    }

    /// Whether the young-generation collection that ran last was a tenure.
    pub(crate) fn last_was_tenure(&self) -> bool {
        self.last_tenure.load(Ordering::Relaxed)
    }

    /// Whether the minor about to run should be a tenure; consumes the turn if so.
    pub(crate) fn take_tenure_turn(&self) -> bool {
        let tenure = self
            .left
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
            .is_ok();
        self.last_tenure.store(tenure, Ordering::Relaxed);
        tenure
    }

    /// The reference major yield, for diagnostics: (freed bytes, pause µs).
    pub(crate) fn major_reference(&self) -> (u64, u64) {
        (self.major_freed.load(Ordering::Relaxed), self.major_pause_us.load(Ordering::Relaxed))
    }

    /// Current tenure run length, for diagnostics.
    pub(crate) fn run(&self) -> u32 {
        self.run.load(Ordering::Relaxed)
    }
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// Judge the real minor that just ran (`live_after`: where it left the heap — the minor gate
    /// is read off it, the same gate the next trip will use).
    pub(super) fn observe_minor_yield(&self, freed: u64, pause_us: u64, live_after: u64, entries: MinorEntries) {
        let gate = self.minor_gate(live_after, self.soft_cap());
        let verdict = self.young_policy.observe_minor(MinorYield {
            freed,
            pause_us,
            gate,
            scanned: entries.scanned,
            reclaimed: entries.reclaimed,
            survivors: entries.survivors,
            survivor_budget: self.pause_budget.survivor_budget(self.nursery_bytes.load(Ordering::Relaxed)),
        });
        if verdict != Verdict::Productive {
            let (mf, mp) = self.young_policy.major_reference();
            crate::gc::phase_timer::note(format_args!(
                "yield  minor {} in {pause_us} us  {verdict:?} (major {} in {mp} us)  -> tenure x{}",
                crate::gc::trace::human(freed), crate::gc::trace::human(mf), self.young_policy.run()));
        }
    }

    /// Fold one pause of major work into the policy. A completed major is futile when it freed
    /// less than `allowance floor / FUTILE_DIVISOR` (the floor: the allowance a small heap gets).
    pub(super) fn observe_major_work(&self, freed: u64, pause_us: u64, finished: bool) {
        let bar = self.allowance_floor() / FUTILE_DIVISOR;
        self.young_policy.observe_major_work(freed, pause_us, finished, bar);
    }

    /// **Tenure** the young generation: every young entry and block becomes old at the promotion
    /// age, with no mark, no card scan and no sweep. The caller holds the pause. Returns the bytes
    /// moved into the old generation (counted towards the next major's trigger).
    ///
    /// Why nothing needs a card afterwards: the write barrier and promotion keep "an old entry
    /// pointing at a young one has a dirty card", and after a tenure no young entry is left —
    /// except, while a cycle sweeps, entries without its epoch, which are garbage (marking is
    /// complete) and so are pointed at by nothing live.
    pub(super) fn run_tenure(&self) -> u64 {
        // Same as a minor: merge this thread's TLAB so its entries are listed (every other mutator
        // retired at its park).
        self.retire_thread_tlab();
        let doomed_unless = self.doomed_unless_marked();
        let (arrs, arr_bytes) = self
            .region_array
            .lock()
            .tenure_young(doomed_unless, |e| Self::array_size_estimate(&e.value));
        // Object model R6: objects are var blocks; the var tenure reports their bytes.
        let (blocks, obj_bytes) = self.region_var.lock().tenure_young(doomed_unless);
        let promoted = obj_bytes + arr_bytes;
        self.promoted_bytes_since_major.fetch_add(promoted, Ordering::Relaxed);
        crate::gc::phase_timer::note(format_args!(
            "tenure  arrays {arrs}  var blocks (objects included) {blocks}  -> old {}  (run {})",
            crate::gc::trace::human(promoted), self.young_policy.run()));
        promoted
    }
}

#[cfg(test)]
#[path = "young_policy_tests.rs"]
mod young_policy_tests;
