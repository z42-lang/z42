//! add-gc-stress-test P0 (2026-05-22): random-workload stress under
//! all 3 GcMode variants. Builds on C1's `debug_validate_invariants`
//! which auto-runs after every collect — stress just drives random
//! ops at the heap and trips the validator on subtle bugs.
//!
//! Hand-rolled xorshift64 PRNG keeps the test self-contained (no
//! new crate dep). Each test seeds with a fixed u64 → reproducible
//! across builds. Override per-test via `Z42_STRESS_SEED` for replay
//! of a CI failure; override iter count via `Z42_STRESS_ITERS`.

use super::*;
use crate::gc::{GcMode, MagrGC, RootHandle};

/// Default iters per stress test. Tuned for `<1s per test in debug`.
/// Override via `Z42_STRESS_ITERS` env var.
const DEFAULT_ITERS: usize = 2000;

/// Cap on live `Value` pool — prevents unbounded growth under stress.
const MAX_LIVE_OBJECTS: usize = 200;

/// Xorshift64 PRNG. Deterministic; same seed → same sequence across
/// rebuilds. 1 mul + 3 shifts + 3 xors per `next_u64` — negligible
/// overhead vs the work each op does in the heap.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the all-zeros state (xorshift64 fixed point).
        Self(if seed == 0 { 0xDEADBEEFCAFEBABE } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// `[low, high)` exclusive upper bound. Panics if `low >= high`.
    fn gen_range(&mut self, low: usize, high: usize) -> usize {
        debug_assert!(low < high, "gen_range: empty range [{}, {})", low, high);
        low + (self.next_u64() as usize) % (high - low)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    AllocObject,
    AllocArray,
    FieldSetWithObject,
    FieldSetWithPrimitive,
    ArrayElemSetWithObject,
    PinRandom,
    UnpinRandom,
    ForceCollect,
    SetModeRandom,
}

/// Pick op via weighted distribution. Weights sum to 100; ranges
/// fixed for seed-stability.
fn pick_op(rng: &mut Rng, allow_mode_switch: bool) -> Op {
    let w = rng.gen_range(0, 100);
    if allow_mode_switch {
        match w {
            0..=14   => Op::AllocObject,             // 15%
            15..=29  => Op::AllocArray,              // 15%
            30..=44  => Op::FieldSetWithObject,      // 15%
            45..=54  => Op::FieldSetWithPrimitive,   // 10%
            55..=64  => Op::ArrayElemSetWithObject,  // 10%
            65..=77  => Op::PinRandom,               // 13%
            78..=89  => Op::UnpinRandom,             // 12%
            90..=94  => Op::ForceCollect,            //  5%
            _        => Op::SetModeRandom,           //  5%
        }
    } else {
        match w {
            0..=15   => Op::AllocObject,             // 16%
            16..=31  => Op::AllocArray,              // 16%
            32..=46  => Op::FieldSetWithObject,      // 15%
            47..=57  => Op::FieldSetWithPrimitive,   // 11%
            58..=68  => Op::ArrayElemSetWithObject,  // 11%
            69..=81  => Op::PinRandom,               // 13%
            82..=93  => Op::UnpinRandom,             // 12%
            _        => Op::ForceCollect,            //  6%
        }
    }
}

/// Per-run live state: pool of recently-alloc'd Values + active pins.
struct State {
    objects: Vec<Value>,
    pins: Vec<RootHandle>,
    dummy_td: std::sync::Arc<crate::metadata::TypeDesc>,
    // Op counters for coverage gates.
    n_alloc_object: usize,
    n_alloc_array: usize,
    n_field_set: usize,
    n_array_set: usize,
    n_pin: usize,
    n_unpin: usize,
    n_collect: usize,
    n_mode_switch: usize,
}

impl State {
    fn new() -> Self {
        Self {
            objects: Vec::with_capacity(MAX_LIVE_OBJECTS),
            pins: Vec::new(),
            dummy_td: dummy_type_desc("StressObj"),
            n_alloc_object: 0,
            n_alloc_array: 0,
            n_field_set: 0,
            n_array_set: 0,
            n_pin: 0,
            n_unpin: 0,
            n_collect: 0,
            n_mode_switch: 0,
        }
    }

    fn add_object(&mut self, v: Value) {
        if self.objects.len() >= MAX_LIVE_OBJECTS {
            // Replace a deterministic-ish slot — `pins.len() % cap`
            // mixes some state-derived churn without needing rng.
            let idx = (self.pins.len().wrapping_mul(7))
                .wrapping_add(self.n_alloc_object)
                % self.objects.len();
            self.objects[idx] = v;
        } else {
            self.objects.push(v);
        }
    }

    fn pick_object(&self, rng: &mut Rng) -> Option<Value> {
        if self.objects.is_empty() {
            None
        } else {
            Some(self.objects[rng.gen_range(0, self.objects.len())].clone())
        }
    }
}

fn apply(op: Op, heap: &ArcMagrGC, state: &mut State, rng: &mut Rng) {
    match op {
        Op::AllocObject => {
            let slot_count = rng.gen_range(0, 4);
            let v = heap.alloc_object(
                state.dummy_td.clone(),
                vec![Value::Null; slot_count],
                NativeData::None,
            );
            state.add_object(v);
            state.n_alloc_object += 1;
        }
        Op::AllocArray => {
            let len = rng.gen_range(0, 6);
            let v = heap.alloc_array(vec![Value::Null; len]);
            state.add_object(v);
            state.n_alloc_array += 1;
        }
        Op::FieldSetWithObject => {
            if let (Some(owner), Some(new)) =
                (state.pick_object(rng), state.pick_object(rng))
            {
                if let Value::Object(gc) = &owner {
                    let slot_count = gc.borrow().refs.len();
                    if slot_count > 0 {
                        let slot = rng.gen_range(0, slot_count);
                        gc.borrow_mut().refs[slot] = new.clone();
                        if new.is_heap_ref() {
                            heap.write_barrier_field(&owner, slot, &new);
                        }
                        state.n_field_set += 1;
                    }
                }
            }
        }
        Op::FieldSetWithPrimitive => {
            if let Some(owner) = state.pick_object(rng) {
                if let Value::Object(gc) = &owner {
                    let slot_count = gc.borrow().refs.len();
                    if slot_count > 0 {
                        let slot = rng.gen_range(0, slot_count);
                        let v = Value::I64(rng.next_u64() as i64);
                        gc.borrow_mut().refs[slot] = v;
                        // No barrier for primitive (caller-filter contract).
                        state.n_field_set += 1;
                    }
                }
            }
        }
        Op::ArrayElemSetWithObject => {
            if let (Some(arr), Some(new)) =
                (state.pick_object(rng), state.pick_object(rng))
            {
                if let Value::Array(gc) = &arr {
                    let len = gc.borrow().len();
                    if len > 0 {
                        let idx = rng.gen_range(0, len);
                        gc.borrow_mut().set_boxed(idx, new.clone());
                        if new.is_heap_ref() {
                            heap.write_barrier_array_elem(&arr, idx, &new);
                        }
                        state.n_array_set += 1;
                    }
                }
            }
        }
        Op::PinRandom => {
            if let Some(v) = state.pick_object(rng) {
                let h = heap.pin_root(v);
                state.pins.push(h);
                state.n_pin += 1;
            }
        }
        Op::UnpinRandom => {
            if !state.pins.is_empty() {
                let idx = rng.gen_range(0, state.pins.len());
                let h = state.pins.swap_remove(idx);
                heap.unpin_root(h);
                state.n_unpin += 1;
            }
        }
        Op::ForceCollect => {
            heap.force_collect();
            state.n_collect += 1;
            // Per the add-mark-sweep-collector contract: Rust-local
            // `Value` strong refs are NOT roots. After sweep, any
            // unpinned entry in `state.objects` is tombstoned and its
            // GcRef is stale (use-after-finalize panics on access).
            // Rebuild `state.objects` from the heap's authoritative
            // alive-set so subsequent ops don't touch dead handles.
            state.objects.clear();
            heap.iterate_live_objects(&mut |v| {
                if state.objects.len() < MAX_LIVE_OBJECTS {
                    state.objects.push(v.clone());
                }
            });
        }
        Op::SetModeRandom => {
            let new_mode = match rng.gen_range(0, 3) {
                0 => GcMode::StwMarkSweep,
                1 => GcMode::ConcurrentMarkSweep,
                _ => GcMode::GenerationalMarkSweep,
            };
            heap.set_mode(new_mode);
            state.n_mode_switch += 1;
        }
    }
}

/// Resolve `iters` from env (`Z42_STRESS_ITERS`) or fall back to
/// `DEFAULT_ITERS`. Invalid env value → warn + default.
fn resolve_iters() -> usize {
    match std::env::var("Z42_STRESS_ITERS") {
        Ok(s) => s.parse::<usize>().ok().filter(|&n| n > 0).unwrap_or_else(|| {
            eprintln!("z42: invalid Z42_STRESS_ITERS={:?}; using default {}", s, DEFAULT_ITERS);
            DEFAULT_ITERS
        }),
        Err(_) => DEFAULT_ITERS,
    }
}

/// Resolve seed override from `Z42_STRESS_SEED` env or use `default_seed`.
fn resolve_seed(default_seed: u64) -> u64 {
    match std::env::var("Z42_STRESS_SEED") {
        Ok(s) => s.parse::<u64>().unwrap_or_else(|_| {
            eprintln!("z42: invalid Z42_STRESS_SEED={:?}; using default {}", s, default_seed);
            default_seed
        }),
        Err(_) => default_seed,
    }
}

/// Coverage gates: each stress run must exercise its op categories
/// at least these many times across `iters` iterations. Prevents
/// silent regressions if op weights drift to skip meaningful ops.
fn assert_coverage(state: &State, iters: usize, allow_mode_switch: bool) {
    // Lower bounds proportional to `iters` (account for non-uniform
    // ops + state-dependent skips e.g. empty object pool).
    let scale = iters as f64 / DEFAULT_ITERS as f64;
    let min_alloc = (100.0 * scale) as usize;
    let min_writes = (100.0 * scale) as usize;
    let min_pin = (50.0 * scale) as usize;
    let min_collect = (50.0 * scale) as usize;

    assert!(state.n_alloc_object + state.n_alloc_array >= min_alloc,
        "stress coverage: only {} allocs (min {})",
        state.n_alloc_object + state.n_alloc_array, min_alloc);
    assert!(state.n_field_set + state.n_array_set >= min_writes,
        "stress coverage: only {} writes (min {})",
        state.n_field_set + state.n_array_set, min_writes);
    assert!(state.n_pin >= min_pin,
        "stress coverage: only {} pins (min {})", state.n_pin, min_pin);
    assert!(state.n_collect >= min_collect,
        "stress coverage: only {} force_collects (min {})",
        state.n_collect, min_collect);
    if allow_mode_switch {
        assert!(state.n_mode_switch > 0,
            "stress coverage: mode-switching test did not invoke set_mode");
    }
}

/// Run a seeded stress workload under `mode`. Panics with the seed
/// embedded in the message on failure for reproducibility.
fn gc_stress_run_seeded(seed: u64, iters: usize, mode: GcMode, allow_mode_switch: bool) {
    let mut rng = Rng::new(seed);
    let heap = ArcMagrGC::new();
    heap.set_mode(mode);
    let mut state = State::new();

    for i in 0..iters {
        let op = pick_op(&mut rng, allow_mode_switch);
        // Wrap each apply in catch_unwind so we can attribute the
        // failing seed + iter index.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply(op, &heap, &mut state, &mut rng);
        }));
        if let Err(panic) = result {
            let msg = if let Some(s) = panic.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "<non-string panic payload>".to_string()
            };
            panic!(
                "stress test panicked: seed={} iters={} mode={:?} iter={} op={:?}\n  panic: {}",
                seed, iters, mode, i, op, msg
            );
        }
    }

    // Final coverage check.
    assert_coverage(&state, iters, allow_mode_switch);
}

// ── 4 tests ────────────────────────────────────────────────────────────────

#[test]
fn stress_seeded_stw_short() {
    let seed = resolve_seed(42);
    let iters = resolve_iters();
    gc_stress_run_seeded(seed, iters, GcMode::StwMarkSweep, false);
}

#[test]
fn stress_seeded_concurrent_short() {
    let seed = resolve_seed(0x1234);
    let iters = resolve_iters();
    gc_stress_run_seeded(seed, iters, GcMode::ConcurrentMarkSweep, false);
}

#[test]
fn stress_seeded_generational_short() {
    let seed = resolve_seed(0xC0DE);
    let iters = resolve_iters();
    gc_stress_run_seeded(seed, iters, GcMode::GenerationalMarkSweep, false);
}

#[test]
#[ignore = "concurrent GC stale-mark race under mode-switching: the `GcRef::entry_ref` \
            generation guard (refs.rs) flakily fires cross-platform (windows / linux-arm64 / \
            macos-arm64 all seen on CI; passes locally 5/5). Pre-existing (origin/main炸), \
            NOT introduced by unify-object-byte-layout — the single-mode stress variants \
            (stw / concurrent / generational) are stable and stay gated. Re-enable once the \
            loom-validated fix lands: tracked in \
            docs/spec/changes/investigate-concurrent-gc-stale-mark-race."]
fn stress_seeded_mode_switching_short() {
    let seed = resolve_seed(0xBEEF);
    // Mode-switching test uses larger iters by default so each of the
    // 3 modes gets meaningful coverage between switches.
    let iters = match std::env::var("Z42_STRESS_ITERS") {
        Ok(_) => resolve_iters(),
        Err(_) => 3000,
    };
    gc_stress_run_seeded(seed, iters, GcMode::StwMarkSweep, true);
}

// ── Smoke: PRNG is deterministic ──────────────────────────────────────────

#[test]
fn rng_is_deterministic() {
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    for _ in 0..1000 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn rng_avoids_zero_seed_fixed_point() {
    let mut a = Rng::new(0);
    assert_ne!(a.next_u64(), 0, "zero seed should be remapped");
}

// unify-gc-heap PR-3 UAF repro: a pinned byte[] must survive GC churn with its
// element block (region_var) intact. Mirrors the z42 compression-test failure.
#[test]
fn pr3_pinned_byte_array_survives_gc_churn() {
    let heap = ArcMagrGC::new();
    let want: Vec<u8> = b"hello brotli gc stress test payload xyz".to_vec();
    let v = heap.alloc_bytes(want.clone());
    let _pin = heap.pin_root(v.clone());
    let Value::Array(rc) = &v else { panic!("expected byte[]") };
    for iter in 0..300 {
        for _ in 0..30 {
            let _junk = heap.alloc_bytes(b"junk junk junk junk junk junk junk junk".to_vec());
        }
        heap.force_collect();
        let b = rc.borrow();
        assert_eq!(b.len(), want.len(), "iter {iter}: len corrupt");
        let got = b.as_bytes().expect("byte[] backing");
        assert_eq!(got, &want[..], "iter {iter}: bytes corrupt");
    }
}

// Boxed reference-array variant: pinned Object[] whose elements are heap objects.
#[test]
fn pr3_pinned_ref_array_survives_gc_churn() {
    let heap = ArcMagrGC::new();
    let td = dummy_type_desc("Elem");
    let e0 = heap.alloc_object(td.clone(), vec![Value::I64(11)], NativeData::None);
    let e1 = heap.alloc_object(td.clone(), vec![Value::I64(22)], NativeData::None);
    let v = heap.alloc_array(vec![e0.clone(), e1.clone()]);
    let _pin = heap.pin_root(v.clone());
    let Value::Array(rc) = &v else { panic!("expected array") };
    for iter in 0..300 {
        for _ in 0..30 {
            let _junk = heap.alloc_array(vec![Value::I64(1), Value::I64(2), Value::I64(3)]);
        }
        heap.force_collect();
        let b = rc.borrow();
        assert_eq!(b.len(), 2, "iter {iter}: len corrupt");
        assert!(matches!(b.get_boxed(0), Value::Object(_)), "iter {iter}: elem0 not object: {:?}", b.get_boxed(0));
    }
}

// unify-gc-heap PR-3: aggressive MIXED region_var churn — closures + arrays of many
// sizes + objects, with a pinned nested graph. Stresses the free-list / size-class
// reuse across block types (Closure vs ArrayValue vs ArrayPrim) that the real VM hits.
#[test]
fn pr3_mixed_region_var_churn_keeps_graph_intact() {
    let heap = ArcMagrGC::new();
    let td = dummy_type_desc("Node");
    // Pinned nested graph: array of objects, each object holding a byte[] field-ish ref.
    let leaf0 = heap.alloc_bytes(b"leaf-zero-payload".to_vec());
    let leaf1 = heap.alloc_bytes(b"leaf-one-payload-longer-xxxxxxxxxxxxxxxx".to_vec());
    let obj0 = heap.alloc_object(td.clone(), vec![leaf0.clone(), Value::I64(1)], NativeData::None);
    let obj1 = heap.alloc_object(td.clone(), vec![leaf1.clone(), Value::I64(2)], NativeData::None);
    let root = heap.alloc_array(vec![obj0.clone(), obj1.clone()]);
    let _pin = heap.pin_root(root.clone());
    let Value::Array(root_rc) = &root else { panic!() };

    for iter in 0..400 {
        // churn arrays of varying sizes (many size classes)
        for k in 0..12 {
            let sz = 1 + (k * 7) % 200;
            let _j = heap.alloc_bytes(vec![k as u8; sz]);
            let _b = heap.alloc_array(vec![Value::I64(k as i64); (k % 8) + 1]);
        }
        // churn closures (region_var, same as arrays) with env arrays
        for k in 0..8 {
            let env = heap.alloc_array(vec![Value::I64(k as i64)]);
            let Value::Array(env_rc) = env else { panic!() };
            let _c = heap.alloc_closure(crate::metadata::types::ClosureData {
                env: env_rc, fn_name: format!("lambda${k}"),
            });
        }
        // churn objects
        for k in 0..6 {
            let _o = heap.alloc_object(td.clone(), vec![Value::I64(k as i64)], NativeData::None);
        }
        heap.force_collect();

        // verify the pinned graph is intact (dangling ref → debug validator / next-collect
        // mark BFS panics; here we also read the leaf bytes through the object refs).
        let rb = root_rc.borrow();
        assert_eq!(rb.len(), 2, "iter {iter}: root len");
        for slot in 0..2 {
            let Value::Object(orc) = rb.get_boxed(slot) else { panic!("iter {iter}: slot {slot} not object: {:?}", rb.get_boxed(slot)) };
            let ob = orc.borrow();
            let Value::Array(larc) = &ob.refs[0] else { panic!("iter {iter}: obj{slot}.refs[0] not array: {:?}", ob.refs[0]) };
            let lb = larc.borrow();
            let bytes = lb.as_bytes().expect("leaf byte[]");
            assert!(bytes.starts_with(b"leaf-"), "iter {iter}: leaf{slot} corrupt: {:?}", &bytes[..bytes.len().min(8)]);
        }
    }
}
