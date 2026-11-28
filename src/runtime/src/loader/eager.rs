//! Discover and read transitive dependencies during eager program loading.
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
};

use anyhow::{Context, Result};

use super::{
    decode::{decode_program_file_cached, decode_program_bytes_cached}, file_resolver::bytecode_dependencies,
    resolve_namespace, DecodedProgram, FileKind, ImageCache,
};
use crate::pal::fs::Vfs;

/// Preserve the startup order: enqueue each namespace's candidates after the
/// declared file dependencies, then read the next file and queue its dependencies.
pub(crate) fn load_eager_dependencies(
    fs: &dyn Vfs,
    search_dirs: &[PathBuf],
    entry: &DecodedProgram,
    cache: Option<&ImageCache>,
    skip_loaded_namespaces: bool,
    loaded_paths: &mut HashSet<PathBuf>,
    loaded_names: &mut Vec<String>,
) -> Result<Vec<DecodedProgram>> {
    let mut programs = Vec::new();
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
            return Ok(programs);
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
        let decoded = decode_program_file_cached(fs, &path_str, cache)
            .with_context(|| format!("loading dependency `{path_str}`"))?;
        file_queue.extend(decoded.dependencies.iter().map(|dep| dep.file.clone()));
        namespace_queue.extend(decoded.import_namespaces.iter().cloned());
        loaded_names.push(file);
        programs.push(decoded);
    }
}

/// Resolve the named dependency graph across the host callback and its VFS fallback.
pub(super) fn load_resolved_dependencies(
    resolver: &dyn super::FileResolver,
    fs: &dyn Vfs,
    search_dirs: &[PathBuf],
    entry: &DecodedProgram,
    entry_name: Option<&str>,
    cache: Option<&ImageCache>,
    loaded_names: &mut Vec<String>,
) -> Result<Vec<(String, DecodedProgram, Option<u64>)>> {
    let mut queue = VecDeque::from([super::prelude::PRELUDE_FILE.to_owned()]);
    queue.extend(entry.dependencies.iter().map(|dep| dep.file.clone()));
    if let Some(name) = entry_name.filter(|name| name.ends_with(".zbc")) {
        queue.extend(bytecode_dependencies(resolver, name, fs, name)?);
    }
    let mut visited: HashSet<_> = loaded_names.iter().cloned().collect();
    if let Some(package_name) = &entry.package_name {
        visited.insert(format!("{package_name}.zpkg"));
    }
    let mut programs = Vec::new();
    while let Some(name) = queue.pop_front() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let kind = FileKind::for_dependency(&name)?;
        let bytes = resolver
            .resolve(&name, kind)
            .with_context(|| format!("reading dependency `{name}` from the host"))?;
        let (program, size) = if let Some(bytes) = bytes {
            anyhow::ensure!(kind.validate_code(&bytes), "unexpected format for dependency `{name}`");
            let program = decode_program_bytes_cached(&bytes, cache)
                .with_context(|| format!("decoding dependency `{name}` from the host"))?;
            if kind == FileKind::Zbc {
                queue.extend(bytecode_dependencies(resolver, &name, fs, &name)?);
            }
            (program, Some(bytes.len() as u64))
        } else {
            let Some(path) = search_dirs
                .iter()
                .map(|dir| dir.join(&name))
                .find(|path| path.to_str().is_some_and(|path| fs.exists(path)))
            else {
                continue;
            };
            let path = path.to_string_lossy();
            let program = super::decode_program_file(fs, &path)
                .with_context(|| format!("loading dependency `{path}`"))?;
            if kind == FileKind::Zbc {
                queue.extend(bytecode_dependencies(resolver, &name, fs, &path)?);
            }
            (program, fs.file_len(&path).ok())
        };
        queue.extend(program.dependencies.iter().map(|dep| dep.file.clone()));
        loaded_names.push(name.clone());
        programs.push((name, program, size));
    }
    Ok(programs)
}
