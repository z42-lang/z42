//! **Model D** — the incremental major's safety argument, checked over **every** interleaving of a
//! small heap (add-incremental-major-gc M2b, 2026-09-16).
//!
//! ## Why an exhaustive enumerator and not loom
//!
//! Every step here is atomic with respect to every other: a collector slice runs with the world
//! stopped, and a mutator step is the code between two safepoints (a mutator parks — and hands over
//! its SATB records — only at a safepoint). So the only freedom is the **order** of whole steps,
//! and the question "is there an order that breaks an invariant" is answered exactly by enumerating
//! the orders. loom would explore the same orders through one mutex at far higher cost, and only
//! under `--cfg loom`; this runs in the normal test suite. The model states are memoised, which is
//! what makes the ~10^8 raw interleavings tractable.
//!
//! ## The heap
//!
//! ```text
//! slot 0  R  old, pinned root      R.f → H
//! slot 1  Y  young, garbage        (reachable only from D)
//! slot 2  H  old                   H.f → X     (dirty card)
//! slot 3  D  old, garbage          D.f → Y     (a dead old object in a dirty card)
//! slot 4  X  young                 X.f → O
//! slot 5  O  old                   (reachable only through the young X)
//! slot 6..8  free
//! ```
//!
//! Every old object has its own card (the real table is chunk-granular, so this is the finest —
//! most adversarial — table the card invariant allows): the write barrier dirties it when a young
//! reference is stored into the object, promotion dirties it when the promoted object still refers to
//! something young, and a minor cleans it when seeding finds no young child (or the owner doomed).
//!
//! Threads (each a fixed script of atomic steps):
//!
//! - **collector**: open a cycle, then mark / sweep steps (each a slice of one unit of work);
//! - **minor**: two minor collections, each at any point. A minor promotes what it reached from
//!   the roots, registers, dirty cards and (while a cycle is open) the grey set and SATB records
//!   (the model's promotion age is 1). What it did not reach it reclaims — unless the policy
//!   `minor_honors_cycle_marks` (`keep_major`) keeps a young object the open cycle has marked;
//! - **mutator A** (the SATB case): `r0 = R.f; r1 = r0.f; r0.f = null; use r1, r1.f; r1 = null`;
//! - **mutator B** (newborns, weak reads, slot reuse, the card path): `r0 = new; r1 = weak(D);
//!   use r0; use r1; r0 = new; use r0; R.f.f = r0, r0 = null; new` — the newborn stored into H is
//!   then reachable only through H's card, and the last one is garbage from birth.
//!
//! Every dereference — by a mutator, by the marker popping its grey set, by a minor tracing — checks
//! the slot is alive with the handle's generation. After every step, while the cycle is sweeping,
//! every old object reachable from the roots and registers must be marked (the sweep judges it by
//! that bit alone). At the end the collector finishes, and everything still reachable from the
//! roots and registers must be alive. Then one more minor runs, and B's garbage-from-birth must be
//! gone: a cycle may keep its floating garbage for **its own** duration, not hand it to the old
//! generation (P0-16).
//!
//! [`Props::SafetyAndReclaim`] adds a liveness property on top: **every** minor after B's
//! garbage-from-birth was allocated must reclaim it — including a minor inside the open cycle that
//! allocated it black.
//!
//! ## What it discriminates
//!
//! [`FULL`] — the young generation belongs to the minor: a young entry survives a minor only if
//! the minor itself reaches it, and the cycle epoch a newborn carries is only a "born in this
//! cycle" label — is the runtime's policy and green on both safety and reclaim. [`KEEP_MAJOR`] (a
//! minor keeps whatever the open cycle has marked; the runtime's policy before P1-7) is green on
//! safety and fails reclaim. Turning **any one** invariant off
//! produces a counterexample:
//!
//! | off / on | counterexample |
//! |---|---|
//! | SATB barrier | A's `r1` swept while held |
//! | allocate-black | B's newborn swept while held |
//! | grey set + SATB records as minor roots | the marker pops a handle a minor reclaimed |
//! | weak reads refuse doomed objects | B's weak read revives D, then D is swept |
//! | minor card seeding skips doomed entries | the minor traces D → Y after Y's slot was reused |
//! | card barrier on old ← young writes | the newborn B stored into H is reclaimed by a minor |
//! | promotion sets the mark bit without greying | the promoted X is never traced, O is swept while reachable |
//! | (`KEEP_MAJOR`) reclaim | B's garbage, born black, survives a minor inside its cycle |
//! | (`KEEP_MAJOR`) `keep_major` survivors age | B's garbage, born black, is promoted and outlives its cycle |
//! | (tenure) a tenure marks what it promotes | the tenured X is never traced, O is swept while reachable |
//!
//! **Tenure** (M9, `Policy::young`): either young-generation collection may instead promote the
//! whole young set with no trace and no card work. Safety holds for every placement; its garbage
//! outlives the cycle (it is old now), so the final check gives it one more whole cycle. Whether a
//! tenure while sweeping also promotes the unmarked (doomed) young objects makes no difference to
//! safety here — the runtime leaves them young only so that everything it promotes during a sweep
//! carries the epoch, as a minor's promotions do.

use std::collections::HashSet;

const N: usize = 9;
const R: usize = 0;
const Y: usize = 1;
const H: usize = 2;
const D: usize = 3;
const X: usize = 4;
const O: usize = 5;

type Ref = (usize, u32); // (slot, generation)

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Obj {
    alive: bool,
    gen: u32,
    field: Option<Ref>,
    marked: bool,
    old: bool,
    /// The object's card (meaningful only while old).
    dirty: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Phase {
    Idle,
    Marking,
    Sweeping,
}

#[derive(Clone, Copy)]
struct Policy {
    satb: bool,
    alloc_black: bool,
    minor_roots_queues: bool,
    weak_refuses_doomed: bool,
    minor_skips_doomed: bool,
    /// The write barrier dirties an old owner's card when it stores a young reference.
    card_barrier: bool,
    /// Whether a minor keeps — without promoting — a young object the open cycle has marked but the
    /// minor itself did not reach (`keep_major`). Off: the young generation belongs to the minor.
    minor_honors_cycle_marks: bool,
    /// Only with `minor_honors_cycle_marks`: whether a minor ages (here: promotes) an entry kept
    /// **only** by `keep_major`.
    keep_major_ages: bool,
    /// Control: a minor inside an open cycle "promotes black" by setting the mark bit of what it
    /// promotes without greying it — so the marker never traces it.
    promote_marks_without_grey: bool,
    /// What each of the two young-generation collections is: a minor, or a **tenure** (M9: the
    /// whole young set promoted without a mark; while the cycle sweeps, an unmarked young object
    /// is garbage the sweep has not reached and stays young).
    young: [Young; 2],
    /// Control: a tenure inside an open cycle sets the mark bit of what it promotes (without
    /// greying it).
    tenure_marks: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Young {
    Minor,
    Tenure,
}

/// The runtime's policy: the young generation belongs to the minor (P1-7).
const FULL: Policy = Policy {
    satb: true,
    alloc_black: true,
    minor_roots_queues: true,
    weak_refuses_doomed: true,
    minor_skips_doomed: true,
    card_barrier: true,
    minor_honors_cycle_marks: false,
    keep_major_ages: false,
    promote_marks_without_grey: false,
    young: [Young::Minor, Young::Minor],
    tenure_marks: false,
};

/// Control — the runtime's policy before P1-7: a minor inside an open cycle keeps every young entry
/// carrying the cycle epoch.
const KEEP_MAJOR: Policy = Policy { minor_honors_cycle_marks: true, ..FULL };

/// What `explore` asserts. Safety is always checked.
#[derive(Clone, Copy, PartialEq)]
enum Props {
    Safety,
    /// Plus: every minor after B's garbage-from-birth was allocated reclaims it.
    SafetyAndReclaim,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct State {
    objs: [Obj; N],
    phase: Phase,
    grey: Vec<Ref>,
    satb: Vec<Ref>,
    cursor: usize,
    reg: [[Option<Ref>; 2]; 2],
    weak_d: Ref,
    /// B's last allocation, dropped at birth.
    garbage: Option<Ref>,
    pc: [usize; 4], // collector, minor, A, B
}

const MARK_STEPS: usize = 6;
const COLLECTOR_STEPS: usize = 1 + MARK_STEPS + N; // open, mark ×6, sweep ×N
const SCRIPT_LEN: [usize; 4] = [COLLECTOR_STEPS, 2, 5, 8];

type Check = Result<(), String>;

fn obj(old: bool, field: Option<Ref>) -> Obj {
    Obj { alive: true, gen: 0, field, marked: false, old, dirty: false }
}

impl State {
    fn initial() -> Self {
        let mut objs = [Obj { alive: false, gen: 0, field: None, marked: false, old: false, dirty: false }; N];
        objs[R] = obj(true, Some((H, 0)));
        objs[Y] = obj(false, None);
        objs[H] = obj(true, Some((X, 0)));
        objs[D] = obj(true, Some((Y, 0)));
        objs[X] = obj(false, Some((O, 0)));
        objs[O] = obj(true, None);
        // The card invariant holds going in: every old object with a young child is dirty.
        for i in 0..N {
            if let Some(c) = objs[i].field {
                objs[i].dirty = objs[i].old && !objs[c.0].old;
            }
        }
        State {
            objs,
            phase: Phase::Idle,
            grey: Vec::new(),
            satb: Vec::new(),
            cursor: 0,
            reg: [[None; 2]; 2],
            weak_d: (D, 0),
            garbage: None,
            pc: [0; 4],
        }
    }

    fn live(&self, r: Ref) -> bool {
        self.objs[r.0].alive && self.objs[r.0].gen == r.1
    }

    fn deref(&self, r: Ref, who: &str) -> Result<Obj, String> {
        if self.live(r) { Ok(self.objs[r.0]) } else { Err(format!("{who} dereferenced dangling {r:?}")) }
    }

    fn mark(&mut self, r: Ref) {
        if !self.objs[r.0].marked {
            self.objs[r.0].marked = true;
            self.grey.push(r);
        }
    }

    /// A heap write `slot.field = new`, through the SATB barrier and the card barrier.
    fn write(&mut self, p: &Policy, owner: Ref, new: Option<Ref>, who: &str) -> Check {
        let o = self.deref(owner, who)?;
        if p.satb && self.phase == Phase::Marking {
            if let Some(old) = o.field {
                if self.live(old) && !self.objs[old.0].marked {
                    self.satb.push(old);
                }
            }
        }
        if p.card_barrier && o.old && new.is_some_and(|n| !self.objs[n.0].old) {
            self.objs[owner.0].dirty = true;
        }
        self.objs[owner.0].field = new;
        Ok(())
    }

    fn alloc(&mut self, p: &Policy) -> Ref {
        let slot = (0..N).find(|&i| !self.objs[i].alive).expect("model heap full");
        let gen = self.objs[slot].gen + 1;
        let black = p.alloc_black && self.phase != Phase::Idle;
        self.objs[slot] = Obj { alive: true, gen, field: None, marked: black, old: false, dirty: false };
        (slot, gen)
    }

    fn kill(&mut self, slot: usize) {
        self.objs[slot].alive = false;
        self.objs[slot].gen += 1;
        self.objs[slot].field = None;
        self.objs[slot].dirty = false;
    }

    /// The roots and every register.
    fn roots(&self) -> Vec<Ref> {
        let mut roots: Vec<Ref> = vec![(R, 0)];
        roots.extend(self.reg.iter().flatten().flatten().copied());
        roots
    }

    // ── collector ────────────────────────────────────────────────────────────

    fn collector_step(&mut self, step: usize, p: &Policy) -> Check {
        if step == 0 {
            // Open: whiten (new epoch), grey the roots and every register.
            for o in self.objs.iter_mut() {
                o.marked = false;
            }
            self.phase = Phase::Marking;
            for r in self.roots() {
                if self.live(r) {
                    self.mark(r);
                }
            }
            return Ok(());
        }
        match self.phase {
            Phase::Idle => Ok(()),
            Phase::Marking => self.mark_step(),
            Phase::Sweeping => {
                self.sweep_step(p);
                Ok(())
            }
        }
    }

    fn mark_step(&mut self) -> Check {
        if let Some(r) = self.grey.pop() {
            let o = self.deref(r, "marker")?;
            if let Some(c) = o.field {
                self.deref(c, "marker (child)")?;
                self.mark(c);
            }
            return Ok(());
        }
        for r in std::mem::take(&mut self.satb) {
            self.deref(r, "marker (SATB record)")?;
            self.mark(r);
        }
        if self.grey.is_empty() {
            self.phase = Phase::Sweeping;
            self.cursor = 0;
        }
        Ok(())
    }

    fn sweep_step(&mut self, _p: &Policy) {
        if self.cursor < N {
            let i = self.cursor;
            if self.objs[i].alive && !self.objs[i].marked {
                self.kill(i);
            }
            self.cursor += 1;
        }
        if self.cursor >= N {
            self.phase = Phase::Idle;
        }
    }

    fn finish_cycle(&mut self, p: &Policy) -> Check {
        while self.phase != Phase::Idle {
            match self.phase {
                Phase::Marking => self.mark_step()?,
                Phase::Sweeping => self.sweep_step(p),
                Phase::Idle => {}
            }
        }
        Ok(())
    }

    // ── minor ────────────────────────────────────────────────────────────────

    fn minor(&mut self, p: &Policy) -> Check {
        let mut roots = self.roots();
        if p.minor_roots_queues {
            roots.extend(self.grey.iter().copied());
            roots.extend(self.satb.iter().copied());
        }
        // Card seeding: a dirty old object's young child is a root; a card whose object has no
        // young child — or is doomed while sweeping — is cleaned.
        for i in 0..N {
            let o = self.objs[i];
            if !o.alive || !o.old || !o.dirty {
                continue;
            }
            if p.minor_skips_doomed && self.phase == Phase::Sweeping && !o.marked {
                self.objs[i].dirty = false;
                continue;
            }
            match o.field {
                Some(c) if !self.deref(c, "minor card seeding")?.old => roots.push(c),
                _ => self.objs[i].dirty = false,
            }
        }
        let mut kept = [false; N];
        let mut stack = roots;
        while let Some(r) = stack.pop() {
            let o = self.deref(r, "minor trace")?;
            if o.old {
                continue; // old roots are traced through at seeding; old children are not queued
            }
            if kept[r.0] {
                continue;
            }
            kept[r.0] = true;
            if let Some(c) = o.field {
                stack.push(c);
            }
        }
        let open = self.phase != Phase::Idle;
        let mut promoted = Vec::new();
        for i in 0..N {
            let o = self.objs[i];
            if !o.alive || o.old {
                continue;
            }
            if kept[i] {
                self.objs[i].old = true; // survived a minor: promoted (promotion age 1)
                promoted.push(i);
                if p.promote_marks_without_grey && open {
                    self.objs[i].marked = true;
                }
            } else if p.minor_honors_cycle_marks && open && o.marked {
                // keep_major: the open cycle holds it. It survives; whether it ages is the policy.
                if p.keep_major_ages {
                    self.objs[i].old = true;
                    promoted.push(i);
                }
            } else {
                self.kill(i);
            }
        }
        // Promotion does the barrier's job for the old → young edges it creates.
        for i in promoted {
            if let Some(c) = self.objs[i].field {
                if self.live(c) && !self.objs[c.0].old {
                    self.objs[i].dirty = true;
                }
            }
        }
        Ok(())
    }

    /// **Tenure** (M9): every young object becomes old with no trace, no card seeding and no card
    /// dirtied — nothing young is left for a newly-old object to point at, except while sweeping,
    /// where an unmarked young object is garbage and stays young for the sweep to take.
    fn tenure(&mut self, p: &Policy) {
        let open = self.phase != Phase::Idle;
        for i in 0..N {
            let o = self.objs[i];
            if !o.alive || o.old || (self.phase == Phase::Sweeping && !o.marked) {
                continue;
            }
            self.objs[i].old = true;
            if p.tenure_marks && open {
                self.objs[i].marked = true;
            }
        }
    }

    // ── mutators ─────────────────────────────────────────────────────────────

    fn use_reg(&self, m: usize, k: usize) -> Check {
        match self.reg[m][k] {
            Some(r) => self.deref(r, &format!("mutator {} r{k}", ["A", "B"][m])).map(|_| ()),
            None => Ok(()),
        }
    }

    fn mutator_a(&mut self, step: usize, p: &Policy) -> Check {
        match step {
            0 => self.reg[0][0] = self.deref((R, 0), "A")?.field,
            1 => {
                if let Some(h) = self.reg[0][0] {
                    self.reg[0][1] = self.deref(h, "A")?.field;
                }
            }
            2 => {
                if let Some(h) = self.reg[0][0] {
                    self.write(p, h, None, "A")?;
                }
            }
            3 => {
                // use r1 and r1.f — X's old child O is held only through X.
                if let Some(x) = self.reg[0][1] {
                    if let Some(c) = self.deref(x, "mutator A r1")?.field {
                        self.deref(c, "mutator A r1.f")?;
                    }
                }
            }
            _ => self.reg[0][1] = None,
        }
        Ok(())
    }

    fn mutator_b(&mut self, step: usize, p: &Policy) -> Check {
        match step {
            0 | 4 => self.reg[1][0] = Some(self.alloc(p)),
            1 => {
                // Weak read of D: shaded while marking, refused if doomed while sweeping.
                let w = self.weak_d;
                self.reg[1][1] = if !self.live(w) {
                    None
                } else if self.phase == Phase::Sweeping && p.weak_refuses_doomed && !self.objs[w.0].marked {
                    None
                } else {
                    if self.phase == Phase::Marking && p.satb && !self.objs[w.0].marked {
                        self.satb.push(w);
                    }
                    Some(w)
                };
            }
            2 | 5 => self.use_reg(1, 0)?,
            6 => {
                // R.f.f = r0; r0 = null — the newborn is now held only by an old object (the card path).
                if let (Some(h), Some(n)) = (self.deref((R, 0), "B")?.field, self.reg[1][0]) {
                    self.write(p, h, Some(n), "B")?;
                }
                self.reg[1][0] = None;
            }
            7 => self.garbage = Some(self.alloc(p)),
            _ => self.use_reg(1, 1)?,
        }
        Ok(())
    }

    fn step(&mut self, t: usize, p: &Policy) -> Check {
        let s = self.pc[t];
        self.pc[t] += 1;
        match t {
            0 => self.collector_step(s, p),
            1 => match p.young[s] {
                Young::Minor => self.minor(p),
                Young::Tenure => {
                    self.tenure(p);
                    Ok(())
                }
            },
            2 => self.mutator_a(s, p),
            _ => self.mutator_b(s, p),
        }
    }

    /// After every step. While sweeping, every old object reachable from the roots and registers
    /// is marked — the sweep judges it by that bit alone. With `Props::SafetyAndReclaim`, a minor
    /// leaves nothing of B's garbage-from-birth.
    fn check_step(&self, t: usize, props: Props) -> Check {
        if self.phase == Phase::Sweeping {
            let mut stack = self.roots();
            let mut seen = [false; N];
            while let Some(r) = stack.pop() {
                let o = self.deref(r, "sweeping invariant")?;
                if seen[r.0] {
                    continue;
                }
                seen[r.0] = true;
                if o.old && !o.marked {
                    let cursor = self.cursor;
                    return Err(format!("{r:?} is old, reachable and unmarked while sweeping (cursor {cursor})"));
                }
                stack.extend(o.field);
            }
        }
        if t == 1 && props == Props::SafetyAndReclaim {
            if let Some(g) = self.garbage.filter(|&g| self.live(g)) {
                return Err(format!("a minor ({:?}) left garbage-from-birth {g:?} alive", self.phase));
            }
        }
        Ok(())
    }

    /// All scripts done: finish the cycle, then everything reachable must be alive. One more minor,
    /// and B's garbage-from-birth must be gone.
    fn final_check(mut self, p: &Policy) -> Check {
        self.finish_cycle(p)?;
        self.minor(p)?;
        if p.young.contains(&Young::Tenure) {
            // A tenure hands its garbage to the old generation: the next whole cycle takes it.
            self.collector_step(0, p)?;
            self.finish_cycle(p)?;
        }
        if let Some(g) = self.garbage {
            if self.live(g) {
                return Err(format!("garbage-from-birth {g:?} outlived its cycle and the minor after it"));
            }
        }
        let mut stack = self.roots();
        let mut seen = HashSet::new();
        while let Some(r) = stack.pop() {
            let o = self.deref(r, "final reachability")?;
            if seen.insert(r) {
                stack.extend(o.field);
            }
        }
        Ok(())
    }
}

/// Depth-first over every interleaving, memoising visited states. Returns the first counterexample.
fn explore(p: Policy, props: Props) -> Result<usize, String> {
    let mut seen: HashSet<State> = HashSet::new();
    let mut stack = vec![State::initial()];
    while let Some(s) = stack.pop() {
        if !seen.insert(s.clone()) {
            continue;
        }
        let mut any = false;
        for t in 0..4 {
            if s.pc[t] >= SCRIPT_LEN[t] {
                continue;
            }
            any = true;
            let mut next = s.clone();
            next.step(t, &p)
                .and_then(|()| next.check_step(t, props))
                .map_err(|e| format!("{e} (after pcs {:?})", s.pc))?;
            stack.push(next);
        }
        if !any {
            s.final_check(&p)?;
        }
    }
    Ok(seen.len())
}

#[test]
fn every_interleaving_is_safe_and_reclaims_with_the_full_policy() {
    let states = explore(FULL, Props::SafetyAndReclaim).unwrap();
    assert!(states > 10_000, "the model must actually branch ({states} states)");
}

#[test]
fn every_interleaving_is_safe_when_the_minor_keeps_what_the_cycle_marked() {
    explore(KEEP_MAJOR, Props::Safety).unwrap();
}

fn expect_counterexample(p: Policy, props: Props, what: &str) {
    match explore(p, props) {
        Ok(states) => panic!("control ({what}): expected a counterexample, none in {states} states"),
        Err(e) => eprintln!("control ({what}): {e}"),
    }
}

#[test]
fn when_the_minor_keeps_what_the_cycle_marked_a_minor_in_the_cycle_keeps_garbage_from_birth() {
    expect_counterexample(KEEP_MAJOR, Props::SafetyAndReclaim, "keep_major: reclaim");
}

#[test]
fn without_satb_some_interleaving_loses_a_held_object() {
    expect_counterexample(Policy { satb: false, ..FULL }, Props::Safety, "no SATB");
}

#[test]
fn without_allocate_black_some_interleaving_loses_a_newborn() {
    expect_counterexample(Policy { alloc_black: false, ..FULL }, Props::Safety, "no allocate-black");
}

#[test]
fn without_queues_as_minor_roots_the_marker_meets_a_reclaimed_handle() {
    let p = Policy { minor_roots_queues: false, ..FULL };
    expect_counterexample(p, Props::Safety, "queues not minor roots");
}

#[test]
fn without_refusing_doomed_weak_reads_a_revived_object_is_swept() {
    let p = Policy { weak_refuses_doomed: false, ..FULL };
    expect_counterexample(p, Props::Safety, "weak read revives doomed");
}

#[test]
fn without_skipping_doomed_card_entries_a_minor_follows_a_dangling_edge() {
    expect_counterexample(Policy { minor_skips_doomed: false, ..FULL }, Props::Safety, "minor traces doomed");
}

#[test]
fn without_the_card_barrier_a_newborn_stored_in_an_old_object_is_reclaimed() {
    expect_counterexample(Policy { card_barrier: false, ..FULL }, Props::Safety, "no card barrier");
}

#[test]
fn when_promotion_marks_without_greying_an_old_child_of_a_young_object_is_swept() {
    let p = Policy { promote_marks_without_grey: true, ..FULL };
    expect_counterexample(p, Props::Safety, "promote-black without grey");
}

#[test]
fn when_keep_major_survivors_age_the_cycles_floating_garbage_is_promoted() {
    let p = Policy { keep_major_ages: true, ..KEEP_MAJOR };
    expect_counterexample(p, Props::Safety, "keep_major survivors age");
}

/// M9: a tenure in place of either minor, or both — at every point of the cycle.
#[test]
fn every_interleaving_is_safe_when_minors_are_served_as_tenures() {
    for young in [[Young::Tenure, Young::Tenure], [Young::Tenure, Young::Minor], [Young::Minor, Young::Tenure]] {
        explore(Policy { young, ..FULL }, Props::Safety).unwrap();
    }
}

#[test]
fn when_a_tenure_marks_what_it_promotes_an_old_child_of_a_young_object_is_swept() {
    let p = Policy { young: [Young::Tenure, Young::Tenure], tenure_marks: true, ..FULL };
    expect_counterexample(p, Props::Safety, "tenure marks without grey");
}
