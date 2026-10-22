//! Tests for `.zsym` sidecar parsing rejection paths.
//!
//! Round-trip tests (write sidecar in C# → read in Rust → verify content)
//! are covered by golden tests under `src/tests/exception/sidecar_symbols/`.
//! These unit tests focus on Rust-side rejection of malformed / mispurposed
//! files.

use super::zbc_reader::{parse_zbc_sidecar, parse_zpkg_sidecar, read_build_id};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Builds a minimal zbc header with the given flags + a single section directory
/// entry mapping to nothing (just enough to pass header validation).
fn make_zbc_header(major: u16, minor: u16, flags: u16, sec_count: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(16 + (sec_count as usize) * 12);
    v.extend_from_slice(b"ZBC\0");
    v.extend_from_slice(&major.to_le_bytes());
    v.extend_from_slice(&minor.to_le_bytes());
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&sec_count.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes()); // reserved
    v
}

fn make_zpkg_header(major: u16, minor: u16, flags: u16, sec_count: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(16);
    v.extend_from_slice(b"ZPK\0");
    v.extend_from_slice(&major.to_le_bytes());
    v.extend_from_slice(&minor.to_le_bytes());
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&sec_count.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v
}

// ── Rejection paths ──────────────────────────────────────────────────────────

#[test]
fn parse_zbc_sidecar_rejects_non_zbc_magic() {
    let mut bytes = make_zbc_header(1, 3, 0x04, 0);
    bytes[0] = b'X';
    let err = parse_zbc_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("bad magic"), "{err}");
}

// Strict-pin policy (freeze-zbc-v1 / freeze-zpkg-v0): reader accepts
// only major.minor == ZBC_VERSION_* / ZPKG_VERSION_*. Tests that previously used
// pre-freeze "requires N+" semantics have been updated to use the current minor
// where SymOnly / BLID checks are reachable, and to use mismatched minors where
// the strict-pin reject path is being tested.

use crate::metadata::zbc_reader::{ZBC_VERSION_MINOR, ZPKG_VERSION_MINOR};

#[test]
fn parse_zbc_sidecar_rejects_old_minor() {
    let bytes = make_zbc_header(1, ZBC_VERSION_MINOR - 1, 0x04, 0);
    let err = parse_zbc_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("regen via"), "{err}");
}

#[test]
fn parse_zbc_sidecar_rejects_when_symonly_flag_unset() {
    let bytes = make_zbc_header(1, ZBC_VERSION_MINOR, 0x00, 0);
    let err = parse_zbc_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("SymOnly"), "{err}");
}

#[test]
fn parse_zbc_sidecar_rejects_missing_blid() {
    let bytes = make_zbc_header(1, ZBC_VERSION_MINOR, 0x04, 0);
    let err = parse_zbc_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("BLID"), "{err}");
}

#[test]
fn parse_zpkg_sidecar_rejects_non_zpkg_magic() {
    let mut bytes = make_zpkg_header(0, ZPKG_VERSION_MINOR, 0x04, 0);
    bytes[0] = b'X';
    let err = parse_zpkg_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("bad magic"), "{err}");
}

#[test]
fn parse_zpkg_sidecar_rejects_old_minor() {
    let bytes = make_zpkg_header(0, ZPKG_VERSION_MINOR - 1, 0x04, 0);
    let err = parse_zpkg_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("regen via"), "{err}");
}

#[test]
fn parse_zpkg_sidecar_rejects_when_symonly_flag_unset() {
    let bytes = make_zpkg_header(0, ZPKG_VERSION_MINOR, 0x00, 0);
    let err = parse_zpkg_sidecar(&bytes).unwrap_err();
    assert!(err.to_string().contains("SymOnly"), "{err}");
}

// ── read_build_id behaviour ─────────────────────────────────────────────────

#[test]
fn read_build_id_returns_none_when_absent() {
    let bytes = make_zbc_header(1, 3, 0x00, 0);
    assert!(read_build_id(&bytes).is_none());
}

#[test]
fn read_build_id_returns_none_for_too_short_input() {
    let bytes: Vec<u8> = vec![0; 8]; // < 16 bytes header
    assert!(read_build_id(&bytes).is_none());
}
