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
        assert_eq!(entry.type_id.load(Ordering::Relaxed), UNRESOLVED);
        assert_eq!(entry.slot.load(Ordering::Relaxed), UNRESOLVED);
        assert_eq!(entry.fn_idx.load(Ordering::Relaxed), UNRESOLVED);
    }
    assert_eq!(ic.round_robin.load(Ordering::Relaxed), 0);
}

#[test]
fn field_ic_default_all_slots_unresolved() {
    let ic = FieldIC::default();
    use std::sync::atomic::Ordering;
    for entry in &ic.entries {
        assert_eq!(entry.type_id.load(Ordering::Relaxed), UNRESOLVED);
        assert_eq!(entry.slot.load(Ordering::Relaxed), UNRESOLVED);
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
    vcall_ic_install(&ic, 1, 2, 100);
    assert_eq!(vcall_ic_lookup(&ic, 1), Some((2, 100)));
}

#[test]
fn vcall_ic_poly_four_types() {
    let ic = VCallIC::default();
    for t in 1..=4 { vcall_ic_install(&ic, t, t, t * 100); }
    for t in 1..=4 { assert_eq!(vcall_ic_lookup(&ic, t), Some((t, t * 100))); }
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
        assert_eq!(entry.type_id.load(Ordering::Relaxed), UNRESOLVED);
    }
}
