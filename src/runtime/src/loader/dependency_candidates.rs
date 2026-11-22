//! Resolve declared dependencies to candidates before lazy loader setup.

use std::path::PathBuf;
use crate::pal::fs::Vfs;
use super::{LoadedArtifact, lazy::ZpkgCandidate, resolve_namespace};

/// Build the declared-but-not-loaded zpkg candidate set for the lazy loader.
///
/// Sources (in order, deduped by zpkg file name):
///   1. `.zpkg` artifact's `dependencies` (DEPS section)
///   2. `.zbc`  artifact's `import_namespaces` — reverse-lookup into search dirs
///      for zpkgs declaring each namespace
/// Entries already in `initially_loaded` (e.g. eager-loaded z42.core, or AOT-merged
/// deps) are excluded.
pub(crate) fn build_declared_candidates(
    fs: &dyn Vfs,
    user_artifact: &LoadedArtifact,
    search_dirs: &[PathBuf],
    initially_loaded: &[String],
) -> Vec<(String, ZpkgCandidate)> {
    let mut declared: Vec<(String, ZpkgCandidate)> = Vec::new();
    if search_dirs.is_empty() { return declared; }

    let loaded_has = |name: &str| initially_loaded.iter().any(|f| f == name);
    let declared_has = |d: &[(String, _)], name: &str| d.iter().any(|(f, _)| f == name);

    let libs_paths = search_dirs.to_vec();

    // .zpkg dependencies (DEPS): file field authoritative; fall back to the
    // sibling `namespaces` field if the literal filename does not resolve.
    for dep in &user_artifact.dependencies {
        if loaded_has(&dep.file) || declared_has(&declared, &dep.file) { continue; }
        if let Ok(cand) = ZpkgCandidate::build_in_dirs(fs, search_dirs, &dep.file) {
            declared.push((dep.file.clone(), cand));
            continue;
        }
        for ns in &dep.namespaces {
            let Ok(zpkg_paths) = resolve_namespace(fs, ns, &libs_paths) else {
                continue;
            };
            for zpkg_path in zpkg_paths {
                let Some(file_name) = zpkg_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
                else { continue };
                if loaded_has(&file_name) || declared_has(&declared, &file_name) { continue; }
                match ZpkgCandidate::build_in_dirs(fs, search_dirs, &file_name) {
                    Ok(cand) => declared.push((file_name, cand)),
                    // Left as a warning on purpose: a namespace can resolve to several
                    // candidates, so one unreadable file is not yet fatal. The hint
                    // still names the remedy when it is a version mismatch rather
                    // than a corrupt / unrelated file.
                    Err(e)   => match version_mismatch_hint(&file_name, &e) {
                        Some(hint) => tracing::warn!("{hint}"),
                        None => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
                    },
                }
            }
        }
    }

    // .zbc import_namespaces — reverse lookup
    for ns in &user_artifact.import_namespaces {
        let Ok(zpkg_paths) = resolve_namespace(fs, ns, &libs_paths) else {
            continue;
        };
        for zpkg_path in zpkg_paths {
            let Some(file_name) = zpkg_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
            else { continue };
            if loaded_has(&file_name) { continue; }
            if declared_has(&declared, &file_name) { continue; }
            match ZpkgCandidate::build_in_dirs(fs, search_dirs, &file_name) {
                Ok(cand) => declared.push((file_name, cand)),
                // Left as a warning on purpose: a namespace can resolve to several
                // candidates, so one unreadable file is not yet fatal. The hint
                // still names the remedy when it is a version mismatch rather
                // than a corrupt / unrelated file.
                Err(e)   => match version_mismatch_hint(&file_name, &e) {
                    Some(hint) => tracing::warn!("{hint}"),
                    None => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
                },
            }
        }
    }

    declared
}

// fix-version-mismatch-diagnosis : a .zpkg whose format version is
// not this runtime's is **not** a "skip it and carry on" condition. The strict
// pin (see format/versions.rs) means z42vm reads exactly one generation, so
// a mismatched package can never be substituted — warning and continuing boots a
// world with no stdlib and the real failure surfaces far away as a bogus
// `undefined function Std.IO.Environment.GetCommandLineArgs$0` (and on runtimes
// older than the package, as an outright hang). This is exactly the trap a stale
// `.z42` seed + a freshly built tree walks into after a format bump.
//
// So: version mismatches get named at the point of detection, with the command
// that fixes them. Every other read failure keeps its warn-and-continue
// behavior — those really can be optional / unrelated artifacts.
pub(super) fn version_mismatch_hint(file: &str, e: &anyhow::Error) -> Option<String> {
    let m = crate::zpkg::format::as_version_mismatch(e)?;
    Some(format!(
        "`{file}` was built for a different z42 format version than this runtime: {m}.\n\
         z42vm and every .zpkg it loads must be the same generation — there is no \
         cross-version compatibility (strict-pin policy).\n\
         Fix by rebuilding the libraries against this runtime (`{remedy}`), or by \
         running the z42vm that produced them (`Z42_PORTABLE_VM=<matching z42vm>`).",
        remedy = m.remedy()
    ))
}
