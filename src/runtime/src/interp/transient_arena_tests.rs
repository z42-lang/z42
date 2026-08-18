use super::*;
use crate::metadata::types::{PinSourceKind, PinnedViewData, RefKind, StackClosureData};

fn stack_ref(slot: u32) -> TransientPayload {
    TransientPayload::Ref(RefKind::Stack { frame_idx: 0, slot })
}

#[test]
fn alloc_then_with_reads_back() {
    let mut a = TransientArena::default();
    let idx = a.alloc(7, stack_ref(3));
    let got = a
        .with(idx, 7, |p| match p {
            TransientPayload::Ref(RefKind::Stack { slot, .. }) => *slot,
            _ => u32::MAX,
        })
        .unwrap();
    assert_eq!(got, 3);
    assert_eq!(a.allocs, 1);
}

#[test]
fn stale_frame_id_errors() {
    let mut a = TransientArena::default();
    let idx = a.alloc(1, stack_ref(0));
    // Same idx, wrong frame_id → staleness guard fires (no silent UB).
    assert!(a.with(idx, 2, |_| ()).is_err());
    assert!(a.ref_kind(idx, 2).is_err());
}

#[test]
fn out_of_range_idx_errors() {
    let a = TransientArena::default();
    assert!(a.with(99, 1, |_| ()).is_err());
}

#[test]
fn truncate_is_lifo_and_reused_slot_guards_by_frame_id() {
    let mut a = TransientArena::default();
    let base = a.base();
    let _i0 = a.alloc(10, stack_ref(0));
    let _i1 = a.alloc(10, stack_ref(1));
    assert_eq!(a.base(), base + 2);
    a.truncate(base);
    assert_eq!(a.base(), base);
    // Slot 0 reused by a *different* frame → old handle (frame_id=10) is stale.
    let reused = a.alloc(11, stack_ref(9));
    assert_eq!(reused, 0);
    assert!(a.with(0, 10, |_| ()).is_err()); // old frame_id rejected
    assert!(a.with(0, 11, |_| ()).is_ok()); // new frame_id accepted
}

#[test]
fn ref_kind_clone_releases_borrow() {
    let mut a = TransientArena::default();
    let idx = a.alloc(5, TransientPayload::Ref(RefKind::Stack { frame_idx: 4, slot: 2 }));
    let k = a.ref_kind(idx, 5).unwrap();
    match k {
        RefKind::Stack { frame_idx, slot } => {
            assert_eq!((frame_idx, slot), (4, 2));
        }
        _ => panic!("wrong kind"),
    }
}

#[test]
fn scan_roots_skips_leafless_payloads() {
    let mut a = TransientArena::default();
    a.alloc(1, stack_ref(0));
    a.alloc(1, TransientPayload::PinView(PinnedViewData { ptr: 0xdead, len: 4, kind: PinSourceKind::Str }));
    a.alloc(1, TransientPayload::StackClos(StackClosureData { env_idx: 0, fn_name: "f".into() }));
    let mut n = 0usize;
    a.scan_roots(&mut |_v| n += 1);
    // Stack ref / PinView / StackClosure hold no GC leaves → nothing visited.
    assert_eq!(n, 0);
}
