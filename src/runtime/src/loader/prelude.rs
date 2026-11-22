//! Discover and read the standard prelude from the resolved search paths.
use std::path::PathBuf;

use super::{
    dependency_candidates::version_mismatch_hint, load_artifact_cached, ImageCache, LoadedArtifact,
};
use crate::pal::fs::Vfs;

pub(crate) const PRELUDE_FILE: &str = "z42.core.zpkg";

pub(crate) struct LoadedPrelude {
    pub(crate) path: PathBuf,
    pub(crate) canonical_path: PathBuf,
    pub(crate) artifact: LoadedArtifact,
}

fn prelude_candidates(fs: &dyn Vfs, search_dirs: &[PathBuf]) -> Vec<PathBuf> {
    search_dirs
        .iter()
        .map(|dir| dir.join(PRELUDE_FILE))
        .filter(|path| path.to_str().is_some_and(|path| fs.exists(path)))
        .collect()
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
        match load_artifact_cached(fs, &path_str, cache) {
            Ok(artifact) => {
                tracing::debug!("loaded stdlib z42.core from {path_str}");
                return Ok(Some(LoadedPrelude {
                    path,
                    canonical_path,
                    artifact,
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
