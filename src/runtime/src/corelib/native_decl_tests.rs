//! add-native-decl-consistency-check: the stdlib's `[Native("__x")]` declarations and the
//! VM's `BUILTINS` table are two hand-maintained lists that must agree. Before this test the
//! first sign of drift was a load-time `unknown builtin` panic when the mismatched function
//! was first resolved (and only if that code path ran). The test scans `src/libraries/**/*.z42`
//! at `cargo test` time and enforces both directions:
//!
//! * every declared `[Native("__x")]` name must exist in `BUILTINS` (a typo / a removed builtin
//!   fails here, not at runtime);
//! * every `BUILTINS` name must be declared somewhere in the stdlib **or** be on the explicit
//!   allowlist of names that are legitimately never declared in z42 source (emitted directly by
//!   the compiler / used by the VM internally / host-only). Adding a builtin without declaring
//!   it and without extending the allowlist fails here, which keeps the allowlist honest.
//!
//! Only the positional `[Native("__x")]` form is checked: the `[Native(lib=..., entry=...)]`
//! form binds to native extensions (`native/ext.rs`), not to `BUILTINS`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::BUILTINS;

/// Builtins that are intentionally absent from every `[Native]` declaration in `src/libraries`.
/// Keep this list minimal and documented — an entry here is a claim that the name is consumed by
/// something other than stdlib source.
const UNDECLARED_ALLOWLIST: &[(&str, &str)] = &[
    // Emitted directly by the compiler (value boxing lowering), never spelled in stdlib source.
    ("__box_prim",   "compiler-emitted: add-primitive-value-boxing"),
    ("__box_struct", "compiler-emitted: add-struct-value-semantics"),
    // Invoked by the VM itself (boxed-struct `GetHashCode` protocol intercept in vcall_resolve).
    ("__struct_hash_code", "VM-internal: boxed struct GetHashCode"),
    // Host-only surfaces (REPL line editor, wasm virtual filesystem) wired by their hosts.
    ("__repl_readline",       "host-only: z42-repl cdylib"),
    ("__repl_set_completer",  "host-only: z42-repl cdylib"),
    ("__repl_set_key_editor", "host-only: z42-repl cdylib"),
    ("__vfs_enable", "host-only: wasm playground VFS"),
    ("__vfs_mount",  "host-only: wasm playground VFS"),
    // Legacy string primitives retained for `exec_builtin(name, …)` unit tests / embedders
    // (`BUILTINS` is append-only for `BuiltinId` stability, so they are not removed).
    ("__concat",   "legacy: kept for BuiltinId stability"),
    ("__contains", "legacy: kept for BuiltinId stability"),
    ("__len",      "legacy: kept for BuiltinId stability"),
];

fn libraries_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../libraries")
}

fn collect_z42_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_z42_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "z42") {
            out.push(p);
        }
    }
}

/// `[Native("__name")]` occurrences outside line comments. Returns (name, "file:line").
fn declared_natives() -> Vec<(String, String)> {
    let root = libraries_root();
    let mut files = Vec::new();
    collect_z42_files(&root, &mut files);
    assert!(!files.is_empty(), "no .z42 files under {}", root.display());
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        for (ln, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let mut rest = code;
            while let Some(i) = rest.find("[Native(\"") {
                let after = &rest[i + "[Native(\"".len()..];
                if let Some(end) = after.find('"') {
                    let name = &after[..end];
                    if name.starts_with("__") {
                        let rel = f.strip_prefix(&root).unwrap_or(&f).display().to_string();
                        out.push((name.to_string(), format!("{}:{}", rel, ln + 1)));
                    }
                    rest = &after[end..];
                } else {
                    break;
                }
            }
        }
    }
    out
}

#[test]
fn every_declared_native_has_a_builtin() {
    let table: BTreeSet<&str> = BUILTINS.iter().map(|(n, _)| *n).collect();
    let missing: Vec<String> = declared_natives().into_iter()
        .filter(|(name, _)| !table.contains(name.as_str()))
        .map(|(name, at)| format!("{name} (declared at {at})"))
        .collect();
    assert!(missing.is_empty(),
        "stdlib declares [Native] names with no BUILTINS entry (add the Rust builtin or fix the name):\n  {}",
        missing.join("\n  "));
}

#[test]
fn every_builtin_is_declared_or_allowlisted() {
    let declared: BTreeSet<String> = declared_natives().into_iter().map(|(n, _)| n).collect();
    let allow: BTreeSet<&str> = UNDECLARED_ALLOWLIST.iter().map(|(n, _)| *n).collect();
    let undeclared: Vec<&str> = BUILTINS.iter().map(|(n, _)| *n)
        .filter(|n| !declared.contains(*n) && !allow.contains(n))
        .collect();
    assert!(undeclared.is_empty(),
        "BUILTINS entries neither declared via [Native] in src/libraries nor allowlisted \
         (declare them in stdlib, or add to UNDECLARED_ALLOWLIST with a reason):\n  {}",
        undeclared.join("\n  "));
    // The allowlist must not go stale either: an allowlisted name that IS now declared should
    // simply be removed from the allowlist.
    let stale: Vec<&str> = allow.iter().copied().filter(|n| declared.contains(*n)).collect();
    assert!(stale.is_empty(),
        "UNDECLARED_ALLOWLIST entries are now declared in stdlib — drop them from the allowlist:\n  {}",
        stale.join("\n  "));
    // …and must only name real builtins.
    let table: BTreeSet<&str> = BUILTINS.iter().map(|(n, _)| *n).collect();
    let unknown: Vec<&str> = allow.iter().copied().filter(|n| !table.contains(n)).collect();
    assert!(unknown.is_empty(),
        "UNDECLARED_ALLOWLIST names are not BUILTINS at all:\n  {}", unknown.join("\n  "));
}
