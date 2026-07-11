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
    assert_eq!(ZBC_VERSION_MINOR, 26, "zbc minor at 1.26 (add-delegate-metadata: class_flags bit6=delegate)");
}

#[test]
fn zpkg_version_constants_pinned() {
    assert_eq!(ZPKG_VERSION_MAJOR, 0, "zpkg major locked at 0 by freeze-zpkg-v0");
    assert_eq!(ZPKG_VERSION_MINOR, 31, "zpkg minor at 0.31 (drop-tsig-expt: EXPT+TSIG sections removed)");
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
