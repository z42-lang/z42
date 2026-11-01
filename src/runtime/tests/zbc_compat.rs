//! Cross-language zbc decoder contract test.
//!
//! Exercises Rust's `metadata::reader::read_zbc` against real `.zbc`
//! bytes produced by the C# compiler (the golden test artifacts under
//! `tests/golden/run/<name>/source.zbc`). Catches **any** C# opcode /
//! section format drift that breaks Rust decoding — without waiting for
//! `z42 xtask.zpkg test vm` to find it via end-to-end execution.
//!
//! review2 §4 (跨语言契约自动校验) — closes the gap between
//! `ZbcRoundTripTests` (C# write → C# read only) and full e2e (slow).
//!
//! Regenerate the golden zbc files with `z42 xtask.zpkg regen`
//! after every C# compiler change that affects bytecode emission.

use std::fs;
use std::path::PathBuf;

use z42::metadata::reader::read_zbc;

/// Project root resolved from `CARGO_MANIFEST_DIR` (= src/runtime).
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()        // src/
        .parent().unwrap()        // <root>/
        .to_path_buf()
}

/// Resolve a project-root-relative test-case path (`"<category>/<name>"`, e.g.
/// `"classes/class_basic"`) to its compiled `source.zbc` directory.
///
/// run-golden `.zbc` lives in the artifacts mirror, not beside `source.z42`
/// (mirror-build-output-per-component, 2026-06-16): regen writes
/// `artifacts/build/tests/<category>/<name>/source.zbc` (src/ stripped, re-rooted
/// under artifacts/build/ per component).
fn golden_dir(rel: &str) -> PathBuf {
    project_root().join("artifacts/build/tests").join(rel)
}

/// Recursively collect `(<parent-dir-name>, <path-to-source.zbc>)` under `dir`.
fn collect_source_zbc(dir: &std::path::Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_source_zbc(&path, out);
        } else if path.file_name().map(|n| n == "source.zbc").unwrap_or(false) {
            let name = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push((name, path));
        }
    }
}

/// Iterate every test case that ships with a `source.zbc`. Two populations
/// (mirror-build-output-per-component, 2026-06-16):
///   1. Committed byte-baseline goldens under `src/compiler/z42.package/tests/fixtures/zbc-format/` — checked
///      into git, regen overwrites them in place; still read from src.
///   2. Run-goldens — regen-generated, now mirrored per component under
///      `artifacts/build/tests/` (from `src/tests/`) and
///      `artifacts/build/libraries/<lib>/release/tests/` (from `src/libraries/`).
///      Recurse both roots and collect every `source.zbc` (the per-lib stdlib
///      build cache under artifacts/build/libraries uses other filenames, so the
///      `source.zbc` filter picks up only golden cases).
fn each_golden_zbc() -> Vec<(String, PathBuf)> {
    let root = project_root();
    let mut out = Vec::new();

    // (1) Committed byte-baseline goldens (stay in src).
    if let Ok(entries) = fs::read_dir(root.join("src/compiler/z42.package/tests/fixtures/zbc-format")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let zbc = path.join("source.zbc");
            if zbc.is_file() {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                out.push((name, zbc));
            }
        }
    }

    // (2) Run-goldens from the per-component artifacts mirror (regen-on-demand).
    collect_source_zbc(&root.join("artifacts/build/tests"), &mut out);
    collect_source_zbc(&root.join("artifacts/build/libraries"), &mut out);

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ────────────────────────────────────────────────────────────────────────────
// Broad coverage: every golden zbc decodes without error and has plausible
// shape (≥ 1 function, valid string pool indices on instructions).
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn all_golden_zbc_decode() {
    let goldens = each_golden_zbc();
    // The repo only commits 6 source.zbc files; the rest (~50+ total) are
    // regen-on-demand by `z42 xtask.zpkg regen` and excluded from
    // git via .gitignore. CI runs the regen for us; local `cargo test` runs
    // without regen still get useful decoder coverage on the 6 committed
    // fixtures. Emit a notice when below the regen'd-set threshold instead
    // of failing — the test purpose is "every .zbc that EXISTS decodes",
    // not "regen was invoked recently".
    assert!(
        !goldens.is_empty(),
        "no source.zbc fixtures found anywhere under src/tests/ or src/libraries/<lib>/tests/"
    );
    if goldens.len() < 50 {
        eprintln!(
            "note: only {} source.zbc fixtures present (run \
             `z42 xtask.zpkg regen` for full ~50+ set)",
            goldens.len()
        );
    }

    let mut failures = Vec::new();
    for (name, path) in &goldens {
        let bytes = fs::read(path).expect("read .zbc bytes");
        match read_zbc(&bytes) {
            Err(e) => failures.push(format!("{name}: read_zbc failed: {e}")),
            Ok(module) => {
                if module.functions.is_empty() {
                    failures.push(format!("{name}: 0 functions (suspicious)"));
                }
                // Plausibility: every Call/Builtin/StaticGet etc. references a
                // string-pool index within bounds. We only spot-check a few
                // instructions per function to keep the test fast.
                for func in &module.functions {
                    for block in &func.blocks {
                        for instr in block.instructions.iter().take(50) {
                            check_instr_pool_refs(&module, instr, &func.name)
                                .unwrap_or_else(|msg| {
                                    failures.push(format!("{name}::{}: {msg}", func.name));
                                });
                        }
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} golden zbc(s) failed cross-language decode:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
}

/// Light invariant: instructions that name targets via string-pool indices
/// must reference valid pool entries. Ensures opcode parameter layout matches
/// across C# write and Rust read.
fn check_instr_pool_refs(
    module: &z42::metadata::ir::Module,
    instr: &z42::metadata::Instruction,
    _func_name: &str,
) -> Result<(), String> {
    use z42::metadata::Instruction as I;
    let pool_len = module.string_pool.len();
    // A handful of instructions carry pool-indexed names; verify they resolve.
    // (Most instructions carry inlined String fields after `read_zbc`
    // reconstruction, so the pool-index check applies primarily to ConstStr.)
    if let I::ConstStr { idx, .. } = instr {
        if (*idx as usize) >= pool_len {
            return Err(format!(
                "ConstStr idx={} out of pool bounds (pool len={})",
                idx, pool_len
            ));
        }
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Specific structural assertions: pin a few high-traffic golden tests so
// silent regressions in opcode layout get a precise diagnostic, not just a
// generic "decode failed".
//
// Note (2026-05-09 cleanup): the `hello_*` structural tests previously pinned
// against `basic/hello/source.zbc` were removed in commit `6fc6ccb`-era test
// refactor (`basic/hello/` → single-file `basic/hello.z42`); the
// `all_golden_zbc_decode` test below remains the comprehensive backstop.
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn class_basic_zbc_has_classes() {
    let zbc = golden_dir("classes/class_basic").join("source.zbc");
    if !zbc.is_file() {
        // Ride-along: this assertion is opportunistic. If the test is renamed
        // / restructured upstream, fall through silently rather than fail
        // (the broad `all_golden_zbc_decode` test still exercises everything).
        return;
    }
    let bytes = fs::read(&zbc).expect("class_basic/source.zbc");
    let module = read_zbc(&bytes).expect("class_basic decodes");

    assert!(
        !module.classes.is_empty(),
        "class_basic must declare at least one class via TYPE section"
    );
    // Every class has a non-empty simple name.
    for cls in &module.classes {
        assert!(!cls.name.is_empty(), "class name empty");
    }
}
