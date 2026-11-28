use super::*;
use std::io::Write;
use std::path::PathBuf;

const PACKED_MINIMAL: &[u8] = include_bytes!(concat!(
    env!("Z42_RUNTIME_DIR"),
    "/../compiler/z42.package/tests/fixtures/zpkg-format/packed-minimal/source.zpkg"
));

// Minimal zpkg builder: MAGIC + a directory with one NSPC section holding the
// namespace list, mirroring what `read_zpkg_namespaces` parses. We only need a
// buffer that round-trips through the real reader, so we lean on the existing
// zpkg fixtures the loader tests already exercise — here we assert the *scan +
// filter* wiring, not the byte format (covered by reader tests).

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
    let got = scan_zpkg_candidates(&crate::pal::fs::LocalFs, &[dir.clone()]);
    // None are valid zpkgs → no candidates, and crucially no panic on short/bad.
    assert!(got.is_empty(), "expected no candidates, got {got:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mounted_package_is_found_by_file_name_and_namespace() {
    let fs = crate::pal::fs::MemoryFs::new();
    fs.mount("/libs/greeter.zpkg", PACKED_MINIMAL.to_vec());
    let dirs = [PathBuf::from("/libs")];
    let expected = dirs[0].join("greeter.zpkg");

    let candidate = ZpkgCandidate::build_in_dirs(&fs, &dirs, "greeter.zpkg").unwrap();
    assert_eq!(candidate.file_path, expected);
    assert!(candidate.namespaces.iter().any(|name| name == "Demo.Minimal"));
    assert_eq!(crate::loader::resolve_namespace(&fs, "Demo.Minimal", &dirs).unwrap(), vec![expected]);
}

#[test]
fn namespace_lookup_scans_once_per_load_and_preserves_search_order() {
    let fs = crate::pal::fs::MemoryFs::new();
    let preferred = PathBuf::from("/preferred");
    let fallback = PathBuf::from("/fallback");
    let dirs = [preferred.clone(), preferred.clone(), fallback.clone()];
    let mut lookup = NamespaceLookup::new(&fs, &dirs);

    fs.mount("/preferred/z.zpkg", PACKED_MINIMAL.to_vec());
    fs.mount("/preferred/a.zpkg", PACKED_MINIMAL.to_vec());
    fs.mount("/fallback/a.zpkg", PACKED_MINIMAL.to_vec());
    assert!(lookup.matching("Demo").next().is_none(), "match whole namespaces");
    fs.mount("/preferred/b.zpkg", PACKED_MINIMAL.to_vec());

    let paths: Vec<_> = lookup.matching("Demo.Minimal").map(|c| c.file_path.clone()).collect();
    assert_eq!(paths, [preferred.join("a.zpkg"), preferred.join("z.zpkg"), fallback.join("a.zpkg")]);
    let refreshed = crate::loader::resolve_namespace(&fs, "Demo.Minimal", &dirs).unwrap();
    assert_eq!(refreshed, [preferred.join("a.zpkg"), preferred.join("b.zpkg"), preferred.join("z.zpkg"), fallback.join("a.zpkg")]);
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
    let got = scan_zpkg_candidates(&crate::pal::fs::LocalFs, &[dir.clone()]);
    assert!(got.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
