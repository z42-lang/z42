//! Phase 3 S3 (tokenize-ir-and-zbc-bump, 2026-05-09) IdMap unit tests.
//!
//! IdMap is the v1.0 wire-format token decoder. Pre-1.0 support was dropped
//! in S3c per CLAUDE.md "不为旧版本提供兼容".

use super::*;

fn pool_for_test() -> Vec<String> {
    vec![
        "Demo.Aaa".to_owned(),     // pool[0]
        "Std.IO.Print".to_owned(), // pool[1]
        "Std.Math.Abs".to_owned(), // pool[2]
    ]
}

#[test]
fn local_token_uses_local_funcs_table() {
    let pool = pool_for_test();
    let id_map = IdMap::for_v1(
        &pool,
        vec!["Demo.Main".to_owned(), "Demo.Helper".to_owned()],
        vec!["Demo.Foo".to_owned()],
    );

    // token < IMPORT_BASE → local index.
    assert_eq!(id_map.resolve_method(0).unwrap(), "Demo.Main");
    assert_eq!(id_map.resolve_method(1).unwrap(), "Demo.Helper");
}

#[test]
fn import_token_uses_pool() {
    let pool = pool_for_test();
    let id_map = IdMap::for_v1(
        &pool,
        vec!["Demo.Main".to_owned()],
        vec![],
    );

    // token >= IMPORT_BASE → pool[token - IMPORT_BASE].
    assert_eq!(id_map.resolve_method(IMPORT_BASE_TOKEN + 1).unwrap(), "Std.IO.Print");
    assert_eq!(id_map.resolve_method(IMPORT_BASE_TOKEN + 2).unwrap(), "Std.Math.Abs");
}

#[test]
fn local_class_token_uses_local_classes_table() {
    let pool = pool_for_test();
    let id_map = IdMap::for_v1(
        &pool,
        vec![],
        vec!["Demo.Foo".to_owned(), "Demo.Bar".to_owned()],
    );

    assert_eq!(id_map.resolve_type(0).unwrap(), "Demo.Foo");
    assert_eq!(id_map.resolve_type(1).unwrap(), "Demo.Bar");
}

#[test]
fn unresolved_token_returns_diagnostic_placeholder() {
    let pool = pool_for_test();
    let id_map = IdMap::for_v1(&pool, vec![], vec![]);

    // UNRESOLVED is encoded as 0xFFFF_FFFF, separately handled (not a pool idx).
    assert_eq!(id_map.resolve_method(UNRESOLVED_TOKEN).unwrap(), "<unresolved>");
    assert_eq!(id_map.resolve_type(UNRESOLVED_TOKEN).unwrap(), "<unresolved>");
}

#[test]
fn local_token_oob_errors() {
    let pool = pool_for_test();
    let id_map = IdMap::for_v1(
        &pool,
        vec!["only_one".to_owned()],
        vec![],
    );
    assert!(id_map.resolve_method(99).is_err()); // local OOB
}

#[test]
fn import_token_to_oob_pool_errors() {
    let pool = pool_for_test();
    let id_map = IdMap::for_v1(&pool, vec![], vec![]);

    // IMPORT_BASE + huge → pool OOB
    let bad = IMPORT_BASE_TOKEN + 999;
    assert!(bad < UNRESOLVED_TOKEN); // not the UNRESOLVED sentinel
    assert!(id_map.resolve_method(bad).is_err());
}

// ── Strict-pin version invariants (freeze-zbc-v1 + freeze-zpkg-v0) ────────────
//
// Mirror of C# Z42.Tests.Zbc.FormatInvariantTests + Z42.Tests.Zpkg.FormatInvariantTests.
// Constructs minimal byte streams to exercise reader version checks without
// needing a fully-formed zbc / zpkg (those would require compiling z42 source).

fn build_zbc_header(major: u16, minor: u16, flags: u16) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[0..4].copy_from_slice(&ZBC_MAGIC);
    data[4..6].copy_from_slice(&major.to_le_bytes());
    data[6..8].copy_from_slice(&minor.to_le_bytes());
    data[8..10].copy_from_slice(&flags.to_le_bytes());
    // sec_count = 0, reserved = 0
    data
}

fn build_zpkg_header(major: u16, minor: u16, flags: u16) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[0..4].copy_from_slice(&ZPKG_MAGIC);
    data[4..6].copy_from_slice(&major.to_le_bytes());
    data[6..8].copy_from_slice(&minor.to_le_bytes());
    data[8..10].copy_from_slice(&flags.to_le_bytes());
    data
}

#[test]
fn zbc_version_constants_pinned() {
    // Sanity: writer's claimed version matches what the reader pins.
    // If this fails, the constants drifted out of sync with C# ZbcWriter.
    assert_eq!(ZBC_VERSION_MAJOR, 1, "zbc major locked at 1 by freeze-zbc-v1");
    assert_eq!(ZBC_VERSION_MINOR, 38, "zbc minor at 1.38 (stabilize-instance-dispatch-keys: instance/static-virtual method keys become primary-bare / non-primary full-signature mangle; wire layout unchanged, only key strings)");
}

#[test]
fn zpkg_version_constants_pinned() {
    assert_eq!(ZPKG_VERSION_MAJOR, 0, "zpkg major locked at 0 by freeze-zpkg-v0");
    assert_eq!(ZPKG_VERSION_MINOR, 43, "zpkg minor at 0.43 (stabilize-instance-dispatch-keys: coupled zbc 1.38)");
}

#[test]
fn zbc_read_rejects_wrong_major() {
    let bytes = build_zbc_header(2, ZBC_VERSION_MINOR, 0);
    let err = read_zbc(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("major 2"), "unexpected error: {msg}");
    assert!(msg.contains("not supported"), "unexpected error: {msg}");
}

#[test]
fn zbc_read_rejects_pre_1_0() {
    let bytes = build_zbc_header(0, 9, 0);
    let err = read_zbc(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("major 0"), "unexpected error: {msg}");
}

#[test]
fn zbc_read_rejects_lower_minor() {
    if ZBC_VERSION_MINOR == 0 { return; }
    let bytes = build_zbc_header(ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR - 1, 0);
    let err = read_zbc(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("minor"), "unexpected error: {msg}");
    assert!(msg.contains("regen via"), "expected regen hint: {msg}");
}

#[test]
fn zbc_read_rejects_higher_minor() {
    let bytes = build_zbc_header(ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR + 1, 0);
    let err = read_zbc(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("minor"), "unexpected error: {msg}");
    assert!(msg.contains("regen via"), "expected regen hint: {msg}");
}

#[test]
fn zbc_sidecar_rejects_wrong_minor() {
    // sidecar requires SymOnly flag (0x04) — set it so we hit the version check first
    let bytes = build_zbc_header(ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR + 1, 0x04);
    let err = parse_zbc_sidecar(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("sidecar"), "unexpected error: {msg}");
    assert!(msg.contains("regen via"), "expected regen hint: {msg}");
}

#[test]
fn zpkg_read_rejects_wrong_major() {
    let bytes = build_zpkg_header(1, ZPKG_VERSION_MINOR, 0x01);
    let err = read_zpkg_modules(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("major 1"), "unexpected error: {msg}");
    assert!(msg.contains("not supported"), "unexpected error: {msg}");
}

#[test]
fn zpkg_read_rejects_lower_minor() {
    if ZPKG_VERSION_MINOR == 0 { return; }
    let bytes = build_zpkg_header(ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR - 1, 0x01);
    let err = read_zpkg_modules(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("minor"), "unexpected error: {msg}");
    assert!(msg.contains("regen via"), "expected regen hint: {msg}");
}

#[test]
fn zpkg_read_rejects_higher_minor() {
    let bytes = build_zpkg_header(ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR + 1, 0x01);
    let err = read_zpkg_modules(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("minor"), "unexpected error: {msg}");
    assert!(msg.contains("regen via"), "expected regen hint: {msg}");
}

#[test]
fn zpkg_sidecar_rejects_wrong_minor() {
    let bytes = build_zpkg_header(ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR + 1, 0x04);
    let err = parse_zpkg_sidecar(&bytes).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("sidecar"), "unexpected error: {msg}");
    assert!(msg.contains("regen via"), "expected regen hint: {msg}");
}

// ── add-struct-value-semantics (zbc 1.31): TYPE-section value-struct layout block ──

/// Build a minimal TYPE section holding one struct class with the given struct
/// layout block (size + reference bitmap). Mirrors the exact `read_type` field
/// order so the reader round-trips it.
fn build_type_section_one_struct(size: u32, ref_leaves: &[(u32, u8)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&1u32.to_le_bytes());        // class count = 1
    b.extend_from_slice(&0u32.to_le_bytes());        // name_idx → pool[0]
    b.extend_from_slice(&u32::MAX.to_le_bytes());    // base_idx = none
    b.extend_from_slice(&0u16.to_le_bytes());        // field count = 0
    b.push(0u8);                                     // type-param count = 0
    b.extend_from_slice(&0u16.to_le_bytes());        // class attr count = 0
    b.push(crate::metadata::bytecode::CLASS_FLAG_STRUCT); // class_flags = struct
    b.push(0u8);                                     // class visibility = public (zbc 1.33, enforce-class-access)
    b.extend_from_slice(&0u16.to_le_bytes());        // static field count = 0
    b.extend_from_slice(&0u16.to_le_bytes());        // interface count = 0
    // (no enum block: not CLASS_FLAG_ENUM; no iface-method block: not INTERFACE)
    // struct block (gated on CLASS_FLAG_STRUCT):
    b.extend_from_slice(&size.to_le_bytes());        // struct size
    b.extend_from_slice(&(ref_leaves.len() as u16).to_le_bytes()); // ref leaf count
    for &(off, kind) in ref_leaves {
        b.extend_from_slice(&off.to_le_bytes());
        b.push(kind);
    }
    b
}

#[test]
fn struct_layout_block_roundtrips_ref_bitmap() {
    // struct { s: string @0; n: int @16 } → size 20, one ArcString ref leaf @0.
    let pool = vec!["Demo.R".to_owned()];
    let sec = build_type_section_one_struct(20, &[(0, crate::metadata::types::STRUCT_REF_ARC_STRING)]);
    let classes = read_type(&sec, &pool).expect("read_type");
    assert_eq!(classes.len(), 1);
    let sl = classes[0].struct_layout.as_ref().expect("struct class must carry a layout block");
    assert_eq!(sl.size, 20);
    assert_eq!(&*sl.ref_offsets, &[0u32]);
    assert_eq!(&*sl.ref_kinds, &[crate::metadata::types::STRUCT_REF_ARC_STRING]);
}

#[test]
fn pure_primitive_struct_block_has_no_ref_leaves() {
    // struct { x: int @0; y: int @4 } → size 8, empty reference bitmap.
    let pool = vec!["Demo.P".to_owned()];
    let sec = build_type_section_one_struct(8, &[]);
    let classes = read_type(&sec, &pool).expect("read_type");
    let sl = classes[0].struct_layout.as_ref().expect("layout present");
    assert_eq!(sl.size, 8);
    assert!(sl.ref_offsets.is_empty(), "pure-primitive struct has no ref leaves");
}

// ── fix-version-mismatch-diagnosis (2026-09-05) ──────────────────────────────
// A version mismatch is now a *typed* error (`FormatVersionMismatch`) so callers
// that warn-and-continue on ordinary read failures can hard-fail on this one.
// These tests pin both halves: the user-visible strings must not drift (they are
// the same text the pre-2026-09-05 inline `bail!`s produced), and the type must
// stay recoverable from an anyhow chain that has been `.context(…)`-wrapped.

#[test]
fn zpkg_wrong_minor_is_a_typed_version_mismatch() {
    let bytes = build_zpkg_header(ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR + 1, 0x01);
    let err = read_zpkg_modules(&bytes).unwrap_err();
    let m = as_version_mismatch(&err).expect("should carry FormatVersionMismatch");
    assert_eq!(m.kind, FormatKind::Zpkg);
    assert_eq!(m.found, (ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR + 1));
    assert_eq!(m.writer, (ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR));
    assert!(!m.major_differs);
    assert_eq!(m.remedy(), "xtask build stdlib");
}

#[test]
fn zbc_wrong_minor_is_a_typed_version_mismatch() {
    let bytes = build_zbc_header(ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR + 1, 0);
    let err = read_zbc(&bytes).unwrap_err();
    let m = as_version_mismatch(&err).expect("should carry FormatVersionMismatch");
    assert_eq!(m.kind, FormatKind::Zbc);
    assert!(!m.major_differs);
    assert_eq!(m.remedy(), "xtask regen");
}

#[test]
fn version_mismatch_survives_context_wrapping() {
    // The real path wraps the reader error in `.context("cannot read zpkg
    // metadata")` (loader/artifact.rs) before app.rs sees it — the downcast must
    // still find it, or the hard-fail silently degrades back to a warning.
    use anyhow::Context;
    let bytes = build_zpkg_header(ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR + 1, 0x01);
    let err = read_zpkg_modules(&bytes)
        .context("cannot read zpkg metadata")
        .context("cannot parse zpkg `x.zpkg`")
        .unwrap_err();
    assert!(as_version_mismatch(&err).is_some(), "downcast lost through context()");
}

#[test]
fn version_mismatch_messages_are_unchanged() {
    // Byte-identical to the strings the inline bail!s produced.
    let zpkg = read_zpkg_modules(&build_zpkg_header(ZPKG_VERSION_MAJOR, 41, 0x01)).unwrap_err();
    assert_eq!(
        as_version_mismatch(&zpkg).unwrap().to_string(),
        format!(
            "zpkg minor 41 not supported (writer is at {ZPKG_VERSION_MAJOR}.{ZPKG_VERSION_MINOR}); \
             regen via xtask build stdlib"
        )
    );
    let zpkg_maj = read_zpkg_modules(&build_zpkg_header(9, ZPKG_VERSION_MINOR, 0x01)).unwrap_err();
    assert_eq!(
        as_version_mismatch(&zpkg_maj).unwrap().to_string(),
        format!("zpkg major 9 not supported (writer is at {ZPKG_VERSION_MAJOR})")
    );
    let zbc = read_zbc(&build_zbc_header(ZBC_VERSION_MAJOR, 7, 0)).unwrap_err();
    assert_eq!(
        as_version_mismatch(&zbc).unwrap().to_string(),
        format!("zbc minor 7 not supported (writer is at {ZBC_VERSION_MINOR}); regen via xtask regen")
    );
}

#[test]
fn ordinary_read_failures_are_not_version_mismatches() {
    // Guard the discrimination the fix depends on: a corrupt/unrelated file must
    // stay a plain error so app.rs keeps warn-and-continue for it.
    let mut bytes = build_zpkg_header(ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR, 0x01);
    bytes[0..4].copy_from_slice(b"XXXX");
    let err = read_zpkg_modules(&bytes).unwrap_err();
    assert!(as_version_mismatch(&err).is_none(), "bad magic must not read as a version mismatch");
}
