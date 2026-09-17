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
//! slot 2  H  old                   H.f → X
//! slot 3  D  old, garbage          D.f → Y     (a dead old object in a dirty card)
//! slot 4  X  young
//! slot 5..7  free
//! ```
//!
//! Threads (each a fixed script of atomic steps):
//!
//! - **collector**: open a cycle, then mark / sweep steps (each a slice of one unit of work);
//! - **minor**: one minor collection, at any point;
//! - **mutator A** (the SATB case): `r0 = R.f; r1 = r0.f; r0.f = null; use r1; r1 = null`;
//! - **mutator B** (newborns, weak reads, slot reuse): `r0 = new; r1 = weak(D); use r0; use r1;
//!   r0 = new; use r0`.
//!
//! Every dereference — by a mutator, by the marker popping its grey set, by a minor tracing — checks
//! the slot is alive with the handle's generation. At the end the collector finishes, and everything
//! still reachable from the roots and registers must be alive.
//!
//! ## What it discriminates
//!
//! Each invariant the implementation relies on is a policy flag; the full policy is green and
//! turning **any one** off produces a counterexample:
//!
//! | off | counterexample |
//! |---|---|
//! | SATB barrier | A's `r1` swept while held |
//! | allocate-black | B's newborn swept while held |
//! | grey set + SATB records as minor roots | the marker pops a handle a minor reclaimed |
//! | weak reads refuse doomed objects | B's weak read revives D, then D is swept |
//! | minor card seeding skips doomed entries | the minor traces D → Y after Y's slot was reused |

use std::collections::HashSet;

const N: usize = 8;
const R: usize = 0;
const Y: usize = 1;
const H: usize = 2;
const D: usize = 3;
const X: usize = 4;

type Ref = (usize, u32); // (slot, generation)

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Obj {
    alive: bool,
    gen: u32,
    field: Option<Ref>,
    marked: bool,
    old: bool,
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
}

const FULL: Policy = Policy {
    satb: true,
    alloc_black: true,
    minor_roots_queues: true,
    weak_refuses_doomed: true,
    minor_skips_doomed: true,
};

#[derive(Clone, PartialEq, Eq, Hash)]
struct State {
    objs: [Obj; N],
    phase: Phase,
    grey: Vec<Ref>,
    satb: Vec<Ref>,
    cursor: usize,
    reg: [[Option<Ref>; 2]; 2],
    weak_d: Ref,
    pc: [usize; 4], // collector, minor, A, B
}

const COLLECTOR_STEPS: usize = 1 + 6 + N; // open, mark ×6, sweep ×N
const SCRIPT_LEN: [usize; 4] = [COLLECTOR_STEPS, 1, 5, 6];

type Check = Result<(), String>;

fn obj(old: bool, field: Option<Ref>) -> Obj {
    Obj { alive: true, gen: 0, field, marked: false, old }
}

impl State {
    fn initial() -> Self {
        let mut objs = [Obj { alive: false, gen: 0, field: None, marked: false, old: false }; N];
        objs[R] = obj(true, Some((H, 0)));
        objs[Y] = obj(false, None);
        objs[H] = obj(true, Some((X, 0)));
        objs[D] = obj(true, Some((Y, 0)));
        objs[X] = obj(false, None);
        State {
            objs,
            phase: Phase::Idle,
            grey: Vec::new(),
            satb: Vec::new(),
            cursor: 0,
            reg: [[None; 2]; 2],
            weak_d: (D, 0),
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

    /// A heap write `slot.field = new`, through the SATB barrier.
    fn write(&mut self, p: &Policy, owner: Ref, new: Option<Ref>, who: &str) -> Check {
        let o = self.deref(owner, who)?;
        if p.satb && self.phase == Phase::Marking {
            if let Some(old) = o.field {
                if self.live(old) && !self.objs[old.0].marked {
                    self.satb.push(old);
                }
            }
        }
        self.objs[owner.0].field = new;
        Ok(())
    }

    fn alloc(&mut self, p: &Policy) -> Ref {
        let slot = (0..N).find(|&i| !self.objs[i].alive).expect("model heap full");
        let gen = self.objs[slot].gen + 1;
        let black = p.alloc_black && self.phase != Phase::Idle;
        self.objs[slot] = Obj { alive: true, gen, field: None, marked: black, old: false };
        (slot, gen)
    }

    fn kill(&mut self, slot: usize) {
        self.objs[slot].alive = false;
        self.objs[slot].gen += 1;
        self.objs[slot].field = None;
    }

    // ── collector ────────────────────────────────────────────────────────────

    fn collector_step(&mut self, step: usize, p: &Policy) -> Check {
        if step == 0 {
            // Open: whiten (new epoch), grey the roots and every register.
            for o in self.objs.iter_mut() {
                o.marked = false;
            }
            self.phase = Phase::Marking;
            self.mark((R, 0));
            for r in self.reg.iter().flatten().flatten().copied().collect::<Vec<_>>() {
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
        let mut roots: Vec<Ref> = vec![(R, 0)];
        roots.extend(self.reg.iter().flatten().flatten().copied());
        if p.minor_roots_queues {
            roots.extend(self.grey.iter().copied());
            roots.extend(self.satb.iter().copied());
        }
        // Card seeding: every alive old object is treated as dirty (a superset of the real card
        // table), its young child a root — except a doomed one while sweeping.
        for i in 0..N {
            let o = self.objs[i];
            if !o.alive || !o.old {
                continue;
            }
            if p.minor_skips_doomed && self.phase == Phase::Sweeping && !o.marked {
                continue;
            }
            if let Some(c) = o.field {
                self.deref(c, "minor card seeding")?;
                roots.push(c);
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
        for i in 0..N {
            if self.objs[i].alive && !self.objs[i].old && !kept[i] {
                self.kill(i);
            }
        }
        Ok(())
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
            3 => self.use_reg(0, 1)?,
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
            _ => self.use_reg(1, 1)?,
        }
        Ok(())
    }

    fn step(&mut self, t: usize, p: &Policy) -> Check {
        let s = self.pc[t];
        self.pc[t] += 1;
        match t {
            0 => self.collector_step(s, p),
            1 => self.minor(p),
            2 => self.mutator_a(s, p),
            _ => self.mutator_b(s, p),
        }
    }

    /// All scripts done: finish the cycle, then everything reachable must be alive.
    fn final_check(mut self, p: &Policy) -> Check {
        self.finish_cycle(p)?;
        let mut stack: Vec<Ref> = vec![(R, 0)];
        stack.extend(self.reg.iter().flatten().flatten().copied());
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
fn explore(p: Policy) -> Result<usize, String> {
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
            next.step(t, &p).map_err(|e| format!("{e} (after pcs {:?})", s.pc))?;
            stack.push(next);
        }
        if !any {
            s.final_check(&p)?;
        }
    }
    Ok(seen.len())
}

#[test]
fn every_interleaving_is_safe_with_the_full_policy() {
    let states = explore(FULL).unwrap();
    assert!(states > 10_000, "the model must actually branch ({states} states)");
}

fn expect_counterexample(p: Policy, what: &str) {
    match explore(p) {
        Ok(states) => panic!("control ({what}): expected a counterexample, none in {states} states"),
        Err(e) => eprintln!("control ({what}): {e}"),
    }
}

#[test]
fn without_satb_some_interleaving_loses_a_held_object() {
    expect_counterexample(Policy { satb: false, ..FULL }, "no SATB");
}

#[test]
fn without_allocate_black_some_interleaving_loses_a_newborn() {
    expect_counterexample(Policy { alloc_black: false, ..FULL }, "no allocate-black");
}

#[test]
fn without_queues_as_minor_roots_the_marker_meets_a_reclaimed_handle() {
    expect_counterexample(Policy { minor_roots_queues: false, ..FULL }, "queues not minor roots");
}

#[test]
fn without_refusing_doomed_weak_reads_a_revived_object_is_swept() {
    expect_counterexample(Policy { weak_refuses_doomed: false, ..FULL }, "weak read revives doomed");
}

#[test]
fn without_skipping_doomed_card_entries_a_minor_follows_a_dangling_edge() {
    expect_counterexample(Policy { minor_skips_doomed: false, ..FULL }, "minor traces doomed");
}
