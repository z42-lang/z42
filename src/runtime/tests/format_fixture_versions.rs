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
//! forgot step 4/9 of `docs/agent/rules/version-bumping.md`" is a red test rather
//! than a silent diff. Regenerate with the per-fixture recipes documented in
//! `src/tests/zpkg-format/README.md`.

use std::path::{Path, PathBuf};

use z42::metadata::zbc_reader::{
    ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR, ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR,
};

fn tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests")
}

/// Header prelude shared by both containers: 4-byte magic, then `major`,
/// `minor` and `flags` as little-endian `u16`s (offsets 4, 6, 8).
fn read_header(path: &Path) -> (String, u16, u16, u16) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(
        bytes.len() >= 10,
        "{} is too short to hold a format header ({} bytes)",
        path.display(),
        bytes.len()
    );
    let magic = String::from_utf8_lossy(&bytes[0..3]).to_string();
    let major = u16::from_le_bytes([bytes[4], bytes[5]]);
    let minor = u16::from_le_bytes([bytes[6], bytes[7]]);
    let flags = u16::from_le_bytes([bytes[8], bytes[9]]);
    (magic, major, minor, flags)
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
        let (magic, major, minor, _flags) = read_header(p);
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
         (docs/agent/rules/version-bumping.md step 4).",
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
         (docs/agent/rules/version-bumping.md step 9).",
    );
}

/// `FlagSymOnly` in the ZPK container header's `flags` u16 — the bit that says
/// "these are `.zsym` debug-symbol sidecar bytes, not a loadable package".
const ZPKG_FLAG_SYM_ONLY: u16 = 0x04;

/// 🔴 **Why this test exists** (found while bumping to zpkg 0.50, 2026-09-27):
/// the version gate above checks only the header's `major.minor`, so a fixture
/// can hold *entirely the wrong kind of file* and stay green forever. That is
/// exactly what had happened: `sym-only-sidecar/source.zpkg` — documented by its
/// own `sym-only-sidecar.z42.toml` as "the `.zsym` sidecar bytes (META + STRS +
/// MDBG + BLID), **not** the main zpkg" — was in fact the **main packed zpkg**
/// (flags = `Packed` only, 8 sections, 535 bytes). A past regen had copied
/// `dist/demo.sidecar.zpkg` instead of `dist/demo.sidecar.zsym`. The sym-only
/// sidecar shape therefore had **zero** byte-level coverage while its gate was
/// green — a gate watching identity (the version number) instead of liveness
/// (is this still the shape it claims to be).
///
/// So: pin the *shape*, not just the version. Cheap, and it cannot rot silently.
#[test]
fn sym_only_fixture_really_holds_sidecar_bytes() {
    let p = tests_root().join("zpkg-format/sym-only-sidecar/source.zpkg");
    let (magic, _major, _minor, flags) = read_header(&p);
    assert_eq!(magic, "ZPK", "{}: unexpected magic {magic:?}", p.display());
    assert_ne!(
        flags & ZPKG_FLAG_SYM_ONLY,
        0,
        "{} does not have FlagSymOnly set (flags = {flags:#06x}) — it is a main \
         package, not the .zsym sidecar this fixture is supposed to freeze.\n\
         Fix: rebuild it from sym-only-sidecar.z42.toml with --release and commit \
         `dist/demo.sidecar.zsym` (NOT dist/demo.sidecar.zpkg) as source.zpkg \
         (see src/tests/zpkg-format/README.md).",
        p.display()
    );
}

// ─── The "唯一真相表" in version-bumping.md ───────────────────────────────────
//
// `docs/agent/rules/version-bumping.md` opens with a four-row table ("版本常量
// 坐标（唯一真相表）") giving, for each of the two writers and the two readers,
// the file, the constant names, and the **current value**. It is the first thing
// anyone consults when bumping a format.
//
// It has rotted twice, and the document says so about itself:
//
//   > ⚠️ 这张表**自己也会腐坏**（2026-09-04 发现时停在 1/35 与 0/40，落后 3 个
//   > minor，且路径在 reader 拆分后已失效）。
//   > 二进制 fixture 那边已有防腐门（步骤 4 / 9），**这张表还没有**。
//
// It then rotted a third time: `type-section-flags2-and-struct-fields` moved all
// four constants to zbc 1.45 / zpkg 0.50 and left the table at 1.44 / 0.49.
//
// That is a gate watching nothing at all: the table is prose, so no build, no
// test and no strict-pin check ever reads it. Meanwhile it is load-bearing —
// a stale row is what sends the next bumper to the wrong file or the wrong
// starting number.
//
// This test closes it the same way the fixture gates above are closed: parse the
// table's own rows and check each claim against **the file that row points at**.
// Path wrong, constant renamed, or value stale ⇒ red.

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `pub const <name>: u16 = <n>;` — the Rust reader side.
fn rust_u16_const(src: &str, name: &str) -> u16 {
    let needle = format!("pub const {name}: u16 = ");
    let at = src
        .find(&needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in versions.rs"));
    let tail = &src[at + needle.len()..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse()
        .unwrap_or_else(|e| panic!("`{name}` value {digits:?} is not a u16: {e}"))
}

/// `public static int <field> = <n>;` inside `public static class <class> {` —
/// the z42 writer side. Scoped to the class so an unrelated `Minor` elsewhere in
/// the file cannot be picked up.
fn z42_static_int(src: &str, class: &str, field: &str) -> u16 {
    let head = format!("public static class {class} ");
    let at = src
        .find(&head)
        .unwrap_or_else(|| panic!("`{head}{{` not found — was the class renamed?"));
    let body = &src[at..];
    let needle = format!("public static int {field} = ");
    let fat = body
        .find(&needle)
        .unwrap_or_else(|| panic!("`{needle}` not found inside `{class}`"));
    let tail = &body[fat + needle.len()..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse()
        .unwrap_or_else(|e| panic!("`{class}.{field}` value {digits:?} is not a u16: {e}"))
}

/// The first backticked span in a markdown table cell (the table puts the path
/// and the constant names in code spans).
fn backticked(cell: &str) -> Option<&str> {
    let a = cell.find('`')? + 1;
    let rest = &cell[a..];
    let b = rest.find('`')?;
    Some(&rest[..b])
}

/// A `| … | … | … | <major> / <minor> |` row of the coordinate table.
struct ClaimRow {
    line_no: usize,
    path: String,
    consts: String,
    claimed: (u16, u16),
}

fn parse_claim_rows(md: &str) -> Vec<ClaimRow> {
    let mut rows = Vec::new();
    for (i, line) in md.lines().enumerate() {
        let t = line.trim();
        if !t.starts_with('|') {
            continue;
        }
        // `| a | b | c | d |` → ["", a, b, c, d, ""]
        let parts: Vec<&str> = t.split('|').collect();
        if parts.len() != 6 {
            continue;
        }
        // Last cell must read exactly "<n> / <n>" — that is what makes it a
        // value-bearing row rather than the header or the `|---|` separator.
        let (lhs, rhs) = match parts[4].split_once('/') {
            Some(p) => p,
            None => continue,
        };
        let (major, minor) = match (lhs.trim().parse::<u16>(), rhs.trim().parse::<u16>()) {
            (Ok(a), Ok(b)) => (a, b),
            _ => continue,
        };
        let path = match backticked(parts[2]) {
            Some(p) => p.to_string(),
            None => continue,
        };
        rows.push(ClaimRow {
            line_no: i + 1,
            path,
            consts: parts[3].to_string(),
            claimed: (major, minor),
        });
    }
    rows
}

#[test]
fn version_bumping_coordinate_table_matches_the_real_constants() {
    let root = repo_root();
    let md_rel = "docs/agent/rules/version-bumping.md";
    let md_path = root.join(md_rel);
    let md = std::fs::read_to_string(&md_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", md_path.display()));

    let rows = parse_claim_rows(&md);
    assert_eq!(
        rows.len(),
        4,
        "{md_rel}: expected the 4-row 「版本常量坐标（唯一真相表）」 table, found {} \
         value-bearing row(s).\n\
         Rows are recognised by a last cell of the exact form `<major> / <minor>`. \
         If you restructured the table, update this gate in the same commit — \
         silently dropping a row is exactly the rot it exists to catch.",
        rows.len()
    );

    // Which constant pair does each row claim to describe? Keyed on the constant
    // names the row itself prints, so renaming a constant without touching the
    // table is also caught.
    let mut seen: Vec<&str> = Vec::new();
    for row in &rows {
        let file = root.join(&row.path);
        assert!(
            file.is_file(),
            "{md_rel}:{}: the table points at `{}`, which does not exist.\n\
             (The paths went stale once before, when the Rust reader was split \
             out of `zbc_reader.rs` into `zbc_reader/versions.rs`.)",
            row.line_no,
            row.path
        );
        let src = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));

        let (kind, actual) = if row.consts.contains("ZbcVersion") {
            ("zbc writer", (z42_static_int(&src, "ZbcVersion", "Major"), z42_static_int(&src, "ZbcVersion", "Minor")))
        } else if row.consts.contains("ZpkgWriterZ") {
            ("zpkg writer", (z42_static_int(&src, "ZpkgWriterZ", "Major"), z42_static_int(&src, "ZpkgWriterZ", "Minor")))
        } else if row.consts.contains("ZBC_VERSION") {
            ("zbc reader", (rust_u16_const(&src, "ZBC_VERSION_MAJOR"), rust_u16_const(&src, "ZBC_VERSION_MINOR")))
        } else if row.consts.contains("ZPKG_VERSION") {
            ("zpkg reader", (rust_u16_const(&src, "ZPKG_VERSION_MAJOR"), rust_u16_const(&src, "ZPKG_VERSION_MINOR")))
        } else {
            panic!(
                "{md_rel}:{}: cannot tell which constants this row describes from {:?}. \
                 Expected one of ZbcVersion / ZpkgWriterZ / ZBC_VERSION / ZPKG_VERSION.",
                row.line_no, row.consts
            );
        };
        assert!(
            !seen.contains(&kind),
            "{md_rel}:{}: two rows both describe the {kind} constants.",
            row.line_no
        );
        seen.push(kind);

        assert_eq!(
            row.claimed, actual,
            "{md_rel}:{}: the table says the {kind} is at {}.{}, but `{}` actually \
             declares {}.{}.\n\
             Fix the table row (step: the 「版本常量坐标」 table is part of every \
             bump, same as the fixture regens in steps 4 / 9).",
            row.line_no, row.claimed.0, row.claimed.1, row.path, actual.0, actual.1
        );
    }
    assert_eq!(seen.len(), 4, "{md_rel}: rows do not cover all four coordinates: {seen:?}");
}

/// strict-pin means the z42 writer and the Rust reader must carry the *same*
/// numbers; a skew makes every artifact unloadable. In practice that shows up
/// instantly (nothing builds), so this is a fast, precise signal rather than a
/// missing gate — it names the two numbers instead of surfacing as
/// `zpkg minor N not supported` from somewhere deep in a build.
#[test]
fn writer_and_reader_pin_the_same_format_versions() {
    let root = repo_root();
    let zbc_w = std::fs::read_to_string(
        root.join("src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42"),
    )
    .expect("read ZbcFormat.z42");
    let zpkg_w =
        std::fs::read_to_string(root.join("src/compiler/z42.package/src/ZpkgWriter.z42"))
            .expect("read ZpkgWriter.z42");

    assert_eq!(
        (
            z42_static_int(&zbc_w, "ZbcVersion", "Major"),
            z42_static_int(&zbc_w, "ZbcVersion", "Minor")
        ),
        (ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR),
        "zbc writer (ZbcFormat.z42) and reader (versions.rs) disagree — under \
         strict-pin every .zbc the writer emits would be rejected."
    );
    assert_eq!(
        (
            z42_static_int(&zpkg_w, "ZpkgWriterZ", "Major"),
            z42_static_int(&zpkg_w, "ZpkgWriterZ", "Minor")
        ),
        (ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR),
        "zpkg writer (ZpkgWriter.z42) and reader (versions.rs) disagree — under \
         strict-pin every .zpkg the writer emits would be rejected."
    );
}
