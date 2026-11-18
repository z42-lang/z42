use super::*;

#[test]
fn unresolved_sentinel_is_u32_max() {
    assert_eq!(UNRESOLVED, u32::MAX);
    assert!(!MethodId::UNRESOLVED.is_resolved());
    assert!(!TypeId::UNRESOLVED.is_resolved());
    assert!(!BuiltinId::UNRESOLVED.is_resolved());
    assert!(!FieldId::UNRESOLVED.is_resolved());
    assert!(!StaticFieldId::UNRESOLVED.is_resolved());
    assert!(!VTableSlot::UNRESOLVED.is_resolved());
}

#[test]
fn resolved_token_reports_resolved() {
    assert!(MethodId(0).is_resolved());
    assert!(MethodId(42).is_resolved());
    assert!(BuiltinId(7).is_resolved());
}

#[test]
fn token_types_are_distinct() {
    // Compile-time check — these would fail to compile if MethodId == TypeId.
    let _m: MethodId = MethodId(1);
    let _t: TypeId = TypeId(1);
    // Cannot mix: `let _: MethodId = TypeId(1);` would not compile.
}

// ── Phase 3 IMPORT_BASE semantics ────────────────────────────────────────

#[test]
fn import_base_constant_is_high_bit() {
    assert_eq!(IMPORT_BASE, 0x8000_0000);
}

#[test]
fn intra_module_token_is_not_import() {
    assert!(!MethodId(0).is_import());
    assert!(!MethodId(42).is_import());
    assert!(!MethodId(0x7FFF_FFFF).is_import());
    assert!(!TypeId(0).is_import());
    assert!(!StaticFieldId(100).is_import());
}

#[test]
fn import_token_reports_import() {
    let m = MethodId(IMPORT_BASE);
    assert!(m.is_import());
    assert_eq!(m.import_idx(), 0);

    let t = TypeId(IMPORT_BASE + 5);
    assert!(t.is_import());
    assert_eq!(t.import_idx(), 5);

    let s = StaticFieldId(IMPORT_BASE + 0x1234);
    assert!(s.is_import());
    assert_eq!(s.import_idx(), 0x1234);
}

#[test]
fn unresolved_is_neither_resolved_nor_import() {
    let u = MethodId::UNRESOLVED;
    assert!(!u.is_resolved());
    assert!(!u.is_import()); // UNRESOLVED specifically excluded from import range
}

#[test]
fn import_token_is_resolved() {
    // An imported token IS resolved (just into import_table, not module.functions);
    // the dispatch site uses is_import() to choose the path.
    assert!(MethodId(IMPORT_BASE).is_resolved());
    assert!(MethodId(IMPORT_BASE + 99).is_resolved());
}

// Gated to debug builds only — `import_idx()` uses `debug_assert!` which is a
// no-op in release, so the panic never fires under `cargo test --release`.
// The test name advertises this scope ("in_debug").
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "import_idx() on non-import token")]
fn import_idx_on_intra_module_panics_in_debug() {
    let m = MethodId(0);
    let _ = m.import_idx();
}
