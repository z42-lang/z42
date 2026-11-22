//! Discover and read transitive dependencies during eager program loading.
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
};

use anyhow::{Context, Result};

use super::{artifact::load_artifact_cached, resolve_namespace, ImageCache, LoadedArtifact};
use crate::pal::fs::Vfs;

/// Preserve the startup order: enqueue each namespace's candidates after the
/// declared file dependencies, then read the next file and queue its dependencies.
pub(crate) fn load_eager_dependencies(
    fs: &dyn Vfs,
    search_dirs: &[PathBuf],
    entry: &LoadedArtifact,
    cache: Option<&ImageCache>,
    skip_loaded_namespaces: bool,
    loaded_paths: &mut HashSet<PathBuf>,
    loaded_names: &mut Vec<String>,
) -> Result<Vec<LoadedArtifact>> {
    let mut artifacts = Vec::new();
    let mut file_queue: VecDeque<_> = entry
        .dependencies
        .iter()
        .map(|dep| dep.file.clone())
        .collect();
    let mut namespace_queue: VecDeque<_> = entry.import_namespaces.iter().cloned().collect();
    loop {
        while let Some(namespace) = namespace_queue.pop_front() {
            if skip_loaded_namespaces && loaded_names.contains(&format!("{namespace}.zpkg")) {
                continue;
            }
            let Ok(paths) = resolve_namespace(fs, &namespace, search_dirs) else {
                continue;
            };
            file_queue.extend(paths.iter().filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            }));
        }

        let Some(file) = file_queue.pop_front() else {
            return Ok(artifacts);
        };
        let Some(path) = search_dirs
            .iter()
            .map(|dir| dir.join(&file))
            .find(|path| path.to_str().is_some_and(|path| fs.exists(path)))
        else {
            continue;
        };
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if loaded_names.contains(&file) || !loaded_paths.insert(canonical) {
            continue;
        }
        let path_str = path.to_string_lossy();
        let artifact = load_artifact_cached(fs, &path_str, cache)
            .with_context(|| format!("loading dependency `{path_str}`"))?;
        file_queue.extend(artifact.dependencies.iter().map(|dep| dep.file.clone()));
        namespace_queue.extend(artifact.import_namespaces.iter().cloned());
        loaded_names.push(file);
        artifacts.push(artifact);
    }
}
