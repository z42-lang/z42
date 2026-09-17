//! Unit tests for `resolver.rs` (per-site token tables + the inline caches).
//!
//! Split out of the inline `#[cfg(test)] mod resolver_tests` when `resolver.rs`
//! crossed the 500-line hard limit — same `#[path]` pattern as
//! `lazy_loader_tests.rs` / `types_tests.rs`.

use super::*;

#[test]
fn resolved_tokens_default_is_empty() {
    let r = ResolvedTokens::default();
    assert!(r.method_tokens.is_empty());
    assert!(r.cross_module_targets.is_empty());
    assert!(r.builtin_tokens.is_empty());
    assert!(r.type_tokens.is_empty());
    assert!(r.vcall_ic.is_empty());
    assert!(r.field_ic.is_empty());
    assert!(r.static_field_tokens.is_empty());
    assert!(r.site_index.is_empty());
}

/// review.md C7 / cache-cross-zpkg-call-target: the per-site cross-zpkg
/// cell contract that `exec_call::call` relies on — empty until first
/// dispatch, write-once fill, borrow-after returns the same `Arc`, and a
/// concurrent/repeat `set` is ignored (so a winner's target stays stable).
#[test]
fn cross_module_target_cell_fill_once_then_borrow() {
    use crate::metadata::bytecode::{BasicBlock, Terminator};
    use crate::metadata::types::ExecMode;

    let mk = |name: &str| {
        Arc::new(Function {
            name: name.to_string(),
            param_count: 0,
            ret_type: "void".to_string(),
            exec_mode: ExecMode::Interp,
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                instructions: Vec::new(),
                terminator: Terminator::Ret { reg: None },
            }],
            is_static: false,
            visibility: 0,
            method_flags: 0, min_arg: 0, params_from: 0xFF,
            max_reg: 0,
            cold: None,
            reg_types: Box::new([]),
            block_index: std::collections::HashMap::new(),
            branch_targets: Vec::new(),
            fused_tails: Vec::new(),
            frame_meta: None,
            resolved: OnceLock::new(),
        })
    };

    let cell: OnceLock<Arc<Function>> = OnceLock::new();
    assert!(cell.get().is_none(), "fresh cell must be empty (forces first-dispatch resolve)");

    let first = mk("Other.zpkg.fn");
    assert!(cell.set(Arc::clone(&first)).is_ok(), "first fill succeeds");
    assert!(Arc::ptr_eq(cell.get().unwrap(), &first), "borrow returns the cached Arc, no re-resolve");

    // A second resolve (e.g. concurrent double-fill) must not replace the
    // cached target — set() returns Err and the original Arc stays.
    let second = mk("Other.zpkg.fn");
    assert!(cell.set(Arc::clone(&second)).is_err(), "repeat fill is rejected (write-once)");
    assert!(Arc::ptr_eq(cell.get().unwrap(), &first), "cached target unchanged after rejected fill");
}

#[test]
fn vcall_ic_default_all_slots_unresolved() {
    let ic = VCallIC::default();
    use std::sync::atomic::Ordering;
    for entry in &ic.entries {
        // 空 entry = (UNRESOLVED, UNRESOLVED) 打包后的 u64（见 ic.rs `pack`）。
        assert_eq!(entry.packed.load(Ordering::Relaxed), u64::MAX);
    }
    assert_eq!(ic.round_robin.load(Ordering::Relaxed), 0);
}

#[test]
fn field_ic_default_all_slots_unresolved() {
    let ic = FieldIC::default();
    use std::sync::atomic::Ordering;
    for entry in &ic.entries {
        assert_eq!(entry.packed.load(Ordering::Relaxed), u64::MAX);
    }
    assert_eq!(ic.round_robin.load(Ordering::Relaxed), 0);
}

// ── PIC lookup + install (review.md C4 P2 + C5 P2) ─────────────────

#[test]
fn field_ic_mono_hit() {
    let ic = FieldIC::default();
    field_ic_install(&ic, 1, 7);
    assert_eq!(field_ic_lookup(&ic, 1), Some(7));
}

#[test]
fn field_ic_poly_two_types_both_hit() {
    let ic = FieldIC::default();
    field_ic_install(&ic, 1, 7);
    field_ic_install(&ic, 2, 9);
    assert_eq!(field_ic_lookup(&ic, 1), Some(7));
    assert_eq!(field_ic_lookup(&ic, 2), Some(9));
}

#[test]
fn field_ic_poly_four_types_all_hit() {
    let ic = FieldIC::default();
    for t in 1..=4 { field_ic_install(&ic, t, t * 10); }
    for t in 1..=4 { assert_eq!(field_ic_lookup(&ic, t), Some(t * 10)); }
}

#[test]
fn field_ic_megamorphic_evicts_via_round_robin() {
    let ic = FieldIC::default();
    for t in 1..=4 { field_ic_install(&ic, t, t * 10); }
    // 5th type triggers round-robin eviction (victim = slot 0).
    field_ic_install(&ic, 5, 50);
    assert_eq!(field_ic_lookup(&ic, 5), Some(50));
    // Three of the original four still present; one (slot 0) replaced.
    let remaining_hits: usize = (1..=4)
        .filter(|t| field_ic_lookup(&ic, *t).is_some())
        .count();
    assert_eq!(remaining_hits, 3, "round-robin victimizes exactly one slot");
}

#[test]
fn field_ic_unresolved_recv_type_returns_none() {
    let ic = FieldIC::default();
    field_ic_install(&ic, 1, 7);
    assert_eq!(field_ic_lookup(&ic, UNRESOLVED), None);
}

#[test]
fn field_ic_install_unresolved_is_noop() {
    let ic = FieldIC::default();
    field_ic_install(&ic, UNRESOLVED, 7);
    // Should not poison the first slot — subsequent install must still hit it.
    field_ic_install(&ic, 1, 9);
    assert_eq!(field_ic_lookup(&ic, 1), Some(9));
}

#[test]
fn vcall_ic_mono_hit() {
    let ic = VCallIC::default();
    vcall_ic_install(&ic, 1, 100);
    assert_eq!(vcall_ic_lookup(&ic, 1), Some(100));
}

#[test]
fn vcall_ic_poly_four_types() {
    let ic = VCallIC::default();
    for t in 1..=4 { vcall_ic_install(&ic, t, t * 100); }
    for t in 1..=4 { assert_eq!(vcall_ic_lookup(&ic, t), Some(t * 100)); }
}

// ── cache-ctorless-objnew ────────────────────────────────────────────────
//
// The mark is a process-global counter, so these tests pass **synthetic**
// live values rather than reading it — otherwise a concurrently-running test
// that loads a package would bump it mid-assertion and make them flaky.

#[test]
fn ctorless_fresh_or_absent_slot_never_hits() {
    use std::sync::atomic::AtomicUsize;
    let slot = AtomicUsize::new(0); // 0 = never proved
    assert!(!ctorless_hit(Some(&slot), 0));
    assert!(!ctorless_hit(Some(&slot), 42));
    // Absent slot = function compiled without a resolved token table; the
    // caller must then always re-resolve.
    assert!(!ctorless_hit(None, 42));
}

#[test]
fn ctorless_hits_only_at_the_mark_it_was_proved_at() {
    use std::sync::atomic::AtomicUsize;
    let slot = AtomicUsize::new(0);
    ctorless_note(Some(&slot), 7);
    assert!(ctorless_hit(Some(&slot), 7), "reusable while nothing registered");
    // Any registration moves the mark — the cached negative must stop hitting,
    // because a function insert is exactly how an absent ctor can appear.
    assert!(!ctorless_hit(Some(&slot), 8));
}

#[test]
fn note_fn_registration_strictly_advances_the_mark() {
    // Holds even if other threads bump concurrently (monotonic counter).
    let before = fn_registration_mark();
    note_fn_registration();
    assert!(fn_registration_mark() > before);
}

#[test]
fn ic_reinstall_same_type_updates_slot_in_place() {
    let ic = FieldIC::default();
    field_ic_install(&ic, 1, 7);
    // Reinstall with the same type should not consume another slot.
    field_ic_install(&ic, 1, 99);
    assert_eq!(field_ic_lookup(&ic, 1), Some(99));
    // And the remaining 3 slots should still be UNRESOLVED.
    use std::sync::atomic::Ordering;
    for entry in &ic.entries[1..] {
        assert_eq!(entry.packed.load(Ordering::Relaxed), u64::MAX);
    }
}


// ── fix-field-ic-publication-race (2026-09-17) ───────────────────────────────

/// (TypeId, 载荷) 必须**成对**发布：并发 install/lookup 期间，任何一次命中拿到的载荷
/// 都必须是该 TypeId 自己的那份，而不是别的 TypeId 的（或还没写入的 `UNRESOLVED`）。
///
/// 这条在修复前是会红的：entry 曾是两个独立 `Relaxed` 原子量，读者能看到
/// 「新 TypeId + 旧/未写入 slot」。压力式测试对内存序竞态**不保证**每次都抓到
/// （x86 上几乎抓不到），确定性的那道门是 `tests/ic_publication_loom.rs`。
#[test]
fn concurrent_install_never_publishes_a_mismatched_payload() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // 约定：type t 的载荷恒为 t * 1000 —— 于是「载荷 != tid * 1000」就是撕裂。
    let ic = Arc::new(FieldIC::default());
    let stop = Arc::new(AtomicBool::new(false));
    let mut writers = Vec::new();
    for t in 1..=8u32 {
        let (ic, stop) = (Arc::clone(&ic), Arc::clone(&stop));
        writers.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                field_ic_install(&ic, t, t * 1000);
            }
        }));
    }
    let mut readers = Vec::new();
    for _ in 0..4 {
        let (ic, stop) = (Arc::clone(&ic), Arc::clone(&stop));
        readers.push(std::thread::spawn(move || {
            let mut torn = 0u64;
            while !stop.load(Ordering::Relaxed) {
                for t in 1..=8u32 {
                    if let Some(payload) = field_ic_lookup(&ic, t) {
                        if payload != t * 1000 { torn += 1; }
                    }
                }
            }
            torn
        }));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    stop.store(true, Ordering::Relaxed);
    for w in writers { w.join().unwrap(); }
    let torn: u64 = readers.into_iter().map(|r| r.join().unwrap()).sum();
    assert_eq!(torn, 0, "PIC 命中拿到了不属于该 TypeId 的载荷（(TypeId, 载荷) 被拆开发布）");
}

/// `vcall` 侧同款不变量。
#[test]
fn vcall_concurrent_install_never_publishes_a_mismatched_payload() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let ic = Arc::new(VCallIC::default());
    let stop = Arc::new(AtomicBool::new(false));
    let mut hs = Vec::new();
    for t in 1..=8u32 {
        let (ic, stop) = (Arc::clone(&ic), Arc::clone(&stop));
        hs.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) { vcall_ic_install(&ic, t, t * 1000); }
            0u64
        }));
    }
    for _ in 0..4 {
        let (ic, stop) = (Arc::clone(&ic), Arc::clone(&stop));
        hs.push(std::thread::spawn(move || {
            let mut torn = 0u64;
            while !stop.load(Ordering::Relaxed) {
                for t in 1..=8u32 {
                    if let Some(p) = vcall_ic_lookup(&ic, t) {
                        if p != t * 1000 { torn += 1; }
                    }
                }
            }
            torn
        }));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    stop.store(true, Ordering::Relaxed);
    let torn: u64 = hs.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(torn, 0, "VCallIC 命中拿到了不属于该 TypeId 的 fn_idx");
}
