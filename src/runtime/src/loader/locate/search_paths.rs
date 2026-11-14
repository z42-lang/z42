//! Ordered search paths from explicit host inputs; no process/environment policy.
use std::path::{Path, PathBuf};
use crate::pal::fs::Vfs;

pub fn entry_directory(file: &str) -> &Path {
    Path::new(file).parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

pub fn program_search_dirs(
    vfs: &dyn Vfs, file: &str, extra: &[PathBuf], libs: Option<&Path>,
) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let entry = entry_directory(file);
    if entry.to_str().is_some_and(|path| vfs.is_dir(path)) { directories.push(entry.to_path_buf()); }
    for path in extra.iter().map(PathBuf::as_path).chain(libs) {
        if !directories.iter().any(|existing| existing == path) { directories.push(path.to_path_buf()); }
    }
    directories
}

#[cfg(test)]
#[path = "search_paths_tests.rs"]
mod tests;
