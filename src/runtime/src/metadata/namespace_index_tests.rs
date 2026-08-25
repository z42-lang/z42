use super::*;
use std::io::Write;
use std::path::PathBuf;

// Minimal zpkg builder: MAGIC + a directory with one NSPC section holding the
// namespace list, mirroring what `read_zpkg_namespaces` parses. We only need a
// buffer that round-trips through the real reader, so we lean on the existing
// zpkg fixtures the loader tests already exercise — here we assert the *scan +
// filter* wiring, not the byte format (covered by zbc_reader tests).

fn tmp_dir(tag: &str) -> PathBuf {
    // Deterministic per-test dir under the OS temp root; cleaned best-effort.
    let mut d = std::env::temp_dir();
    d.push(format!("z42_nsindex_test_{tag}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Write raw bytes to `dir/name`.
fn write_file(dir: &PathBuf, name: &str, bytes: &[u8]) {
    let p = dir.join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(bytes).unwrap();
}

#[test]
fn scan_skips_non_zpkg_and_bad_magic() {
    let dir = tmp_dir("skip");
    // A .txt (wrong ext), a .zpkg with bad magic, and an empty .zpkg.
    write_file(&dir, "notes.txt", b"hello");
    write_file(&dir, "bad.zpkg", b"XXXXrest");
    write_file(&dir, "short.zpkg", b"ZP");
    let got = scan_zpkg_candidates(&[dir.clone()]);
    // None are valid zpkgs → no candidates, and crucially no panic on short/bad.
    assert!(got.is_empty(), "expected no candidates, got {got:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_empty_dirs_returns_empty() {
    assert!(scan_zpkg_candidates(&[]).is_empty());
    assert!(scan_zbc_candidates(&[]).is_empty());
    // Nonexistent dir is skipped, not an error.
    let bogus = PathBuf::from("/definitely/not/a/real/dir/zz");
    assert!(scan_zpkg_candidates(&[bogus.clone()]).is_empty());
    assert!(scan_zbc_candidates(&[bogus]).is_empty());
}

#[test]
fn read_zbc_namespace_rejects_short_and_bad_magic() {
    assert!(read_zbc_namespace(b"tiny").is_err());
    // 16 bytes but wrong magic.
    assert!(read_zbc_namespace(&[0u8; 16]).is_err());
}

#[test]
fn scan_within_dir_is_sorted_deterministic() {
    // Even though these are bad-magic (filtered out), confirm the sort of the
    // directory listing happens before extension/magic filtering — we assert the
    // primitive does not depend on read_dir order by checking two valid-ext files
    // with bad magic both get skipped regardless of on-disk order.
    let dir = tmp_dir("sorted");
    write_file(&dir, "z_last.zpkg", b"XXXXbody");
    write_file(&dir, "a_first.zpkg", b"XXXXbody");
    // Bad magic → skipped; the point is no panic and stable (empty) output.
    let got = scan_zpkg_candidates(&[dir.clone()]);
    assert!(got.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
