//! Anti-rot gate for the committed wire-format byte baselines.
//!
//! `src/tests/zbc-format/` and `src/tests/zpkg-format/` hold check-in'd `.zbc` /
//! `.zpkg` bytes whose purpose is to make wire-format drift visible. Under the
//! pre-1.0 **strict-pin** policy (reader matches writer's major+minor exactly,
//! no compat fallback) a baseline emitted by an older writer is simply dead
//! weight — it can no longer be loaded by the current VM.
//!
//! **Why this test exists.** Nothing used to notice when a format bump forgot to
//! refresh them:
//!
//!   * the `zbc-format` set is regenerated *in place* by `xtask build test`
//!     before any consumer runs, so `zbc_compat` always validated the freshly
//!     rewritten bytes and never the committed ones — stale baselines stayed
//!     green while dirtying every contributor's working tree;
//!   * half the `zpkg-format` set is read by no test at all, so it just rotted
//!     quietly (`sym-only-sidecar` sat 8 minor versions behind).
//!
//! Both were true simultaneously: at the 1.37 → 1.38 bump the six `zbc-format`
//! baselines were left at 1.37, and `packed-multi-module` / `sym-only-sidecar`
//! were left at zpkg 42 / 35 against a writer at 43.
//!
//! This test reads the **committed bytes straight off disk** and asserts their
//! header version equals the current constant, so "someone bumped the format and
//! forgot step 4/9 of `.claude/rules/version-bumping.md`" is a red test rather
//! than a silent diff. Regenerate with the per-fixture recipes documented in
//! `src/tests/zpkg-format/README.md`.

use std::path::{Path, PathBuf};

use z42::metadata::zbc_reader::{
    ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR, ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR,
};

fn tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests")
}

/// Header prelude shared by both containers: 4-byte magic, then `major` and
/// `minor` as little-endian `u16`.
fn read_header(path: &Path) -> (String, u16, u16) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(
        bytes.len() >= 8,
        "{} is too short to hold a format header ({} bytes)",
        path.display(),
        bytes.len()
    );
    let magic = String::from_utf8_lossy(&bytes[0..3]).to_string();
    let major = u16::from_le_bytes([bytes[4], bytes[5]]);
    let minor = u16::from_le_bytes([bytes[6], bytes[7]]);
    (magic, major, minor)
}

/// Every `<dir>/<name>` directly under `src/tests/<category>` that contains
/// `file_name`, sorted for a deterministic failure order.
fn fixtures(category: &str, file_name: &str) -> Vec<PathBuf> {
    let root = tests_root().join(category);
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", root.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path().join(file_name))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
}

fn assert_all(paths: &[PathBuf], want_magic: &str, want_major: u16, want_minor: u16, hint: &str) {
    assert!(
        !paths.is_empty(),
        "found no committed baselines — did the fixture layout move? (looked under {})",
        tests_root().display()
    );
    let mut stale = Vec::new();
    for p in paths {
        let (magic, major, minor) = read_header(p);
        assert_eq!(magic, want_magic, "{}: unexpected magic {magic:?}", p.display());
        if (major, minor) != (want_major, want_minor) {
            stale.push(format!("  {} is {major}.{minor}", p.display()));
        }
    }
    assert!(
        stale.is_empty(),
        "committed byte baselines are stale — the writer is at {want_major}.{want_minor} but:\n{}\n\
         \nA format bump left them behind (strict-pin means the current VM cannot load them).\n{hint}",
        stale.join("\n"),
    );
}

#[test]
fn committed_zbc_baselines_match_the_current_writer() {
    let mut paths = fixtures("zbc-format", "source.zbc");
    // indexed-minimal ships a loose self-contained .zbc alongside its .zpkg.
    paths.extend(fixtures("zpkg-format", "source.zbc"));
    paths.sort();
    assert_all(
        &paths,
        "ZBC",
        ZBC_VERSION_MAJOR,
        ZBC_VERSION_MINOR,
        "Fix: `xtask build compiler && xtask build stdlib && xtask build test` regenerates the \
         zbc-format set in place; review the diff and commit it \
         (.claude/rules/version-bumping.md step 4).",
    );
}

#[test]
fn committed_zpkg_baselines_match_the_current_writer() {
    // `sym-only-sidecar/source.zpkg` holds .zsym sidecar bytes, which share the
    // ZPK container header — same version pin applies.
    let paths = fixtures("zpkg-format", "source.zpkg");
    assert_all(
        &paths,
        "ZPK",
        ZPKG_VERSION_MAJOR,
        ZPKG_VERSION_MINOR,
        "Fix: rebuild each fixture from its committed `<name>.z42.toml` \
         (see src/tests/zpkg-format/README.md) and commit the result \
         (.claude/rules/version-bumping.md step 9).",
    );
}
