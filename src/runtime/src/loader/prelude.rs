//! Discover and read the standard prelude from the resolved search paths.
use anyhow::Context;
use std::path::{Path, PathBuf};

use super::{
    decode::decode_program_file_cached, dependency_candidates::version_mismatch_hint, ImageCache,
    DecodedProgram,
};
use crate::pal::fs::Vfs;

pub(crate) const PRELUDE_FILE: &str = "z42.core.zpkg";

pub(crate) struct LoadedPrelude {
    pub(crate) path: PathBuf,
    pub(crate) canonical_path: PathBuf,
    pub(crate) decoded: DecodedProgram,
}

fn prelude_candidates(fs: &dyn Vfs, search_dirs: &[PathBuf]) -> Vec<PathBuf> {
    search_dirs
        .iter()
        .map(|dir| dir.join(PRELUDE_FILE))
        .filter(|path| path.to_str().is_some_and(|path| fs.exists(path)))
        .collect()
}

/// Validate the first prelude file, if present, before opening an embedding session.
pub(crate) fn probe_prelude(
    fs: &dyn Vfs,
    search_dirs: impl IntoIterator<Item = impl AsRef<Path>>,
) -> anyhow::Result<()> {
    for dir in search_dirs {
        let candidate = dir.as_ref().join(PRELUDE_FILE);
        let Some(path) = candidate.to_str() else { continue; };
        if fs.exists(path) && !fs.is_dir(path) {
            // Validation must leave the process configuration open for an app sidecar.
            super::package::decode_program_file_with_fusion(
                fs, path, || super::fusion_inputs::FusionInputs {
                    no_fusion: true,
                    no_typed_fusion: true,
                    fusion_debug: false,
                }
            ).with_context(|| format!("validating corelib at {path}"))?;
            break;
        }
    }
    Ok(())
}

/// Read the first usable prelude in search order. A version mismatch is fatal
/// only if no later candidate loads, which allows a colocated prelude to serve
/// a different SDK generation during bootstrap.
pub(crate) fn load_prelude(
    fs: &dyn Vfs,
    search_dirs: &[PathBuf],
    cache: Option<&ImageCache>,
    already_resolved: bool,
) -> anyhow::Result<Option<LoadedPrelude>> {
    let mut version_error = None;
    let mut tried_candidate = false;
    for path in prelude_candidates(fs, search_dirs) {
        if already_resolved {
            break;
        }
        tried_candidate = true;
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
        let path_str = path.to_string_lossy();
        match decode_program_file_cached(fs, &path_str, cache) {
            Ok(decoded) => {
                tracing::debug!("loaded stdlib z42.core from {path_str}");
                return Ok(Some(LoadedPrelude {
                    path,
                    canonical_path,
                    decoded,
                }));
            }
            Err(error) => match version_mismatch_hint(&path_str, &error) {
                Some(hint) => {
                    version_error.get_or_insert(hint);
                }
                None => tracing::warn!("failed to load z42.core from {path_str}: {error:#}"),
            },
        }
    }
    if let Some(hint) = version_error {
        anyhow::bail!("{hint}");
    }
    if !tried_candidate {
        tracing::debug!("z42.core.zpkg not found in any of {:?}", search_dirs);
    }
    Ok(None)
}

#[cfg(test)]
#[path = "prelude_tests.rs"]
mod tests;
