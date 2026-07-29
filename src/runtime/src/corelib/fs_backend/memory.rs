//! In-memory filesystem backend — a `path → bytes` map. The default on wasm
//! (no real fs); host code mounts the stdlib + z42c zpkgs via `__vfs_mount`, then
//! `DepScan` / z42c / scripting run **unchanged** (fs.rs routes to the active
//! backend). See `docs/spec/changes/add-wasm-vfs-backend/`.
//!
//! Read/glob/exists are the compile-critical ops (DepScan). Write ops mutate the
//! map; `write_atomic` degrades to a plain write (no fsync in-memory). Directory
//! semantics are path-prefix derived (a dir "exists" iff some file lives under it).
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

fn store() -> &'static RwLock<HashMap<String, Vec<u8>>> {
    static VFS: OnceLock<RwLock<HashMap<String, Vec<u8>>>> = OnceLock::new();
    VFS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Mount a virtual file (path → bytes). Host-facing API — the wasm facade calls
/// this to populate the VFS with stdlib + z42c zpkgs before compiling, so
/// `DepScan`'s `File.ReadAllBytes`/`Path.Glob` (routed here) find them. Also
/// backs the `__vfs_mount` builtin.
pub fn mount(path: &str, bytes: Vec<u8>) {
    store().write().insert(path.to_string(), bytes);
}

pub fn read(path: &str) -> Result<Vec<u8>> {
    store()
        .read()
        .get(path)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("vfs: `{path}` not mounted"))
}
pub fn read_to_string(path: &str) -> Result<String> {
    let bytes = read(path)?;
    Ok(String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("vfs: `{path}` not utf8: {e}"))?)
}
pub fn write(path: &str, bytes: &[u8]) -> Result<()> {
    store().write().insert(path.to_string(), bytes.to_vec());
    Ok(())
}
pub fn append(path: &str, bytes: &[u8]) -> Result<()> {
    let mut s = store().write();
    s.entry(path.to_string()).or_default().extend_from_slice(bytes);
    Ok(())
}
pub fn exists(path: &str) -> bool {
    store().read().contains_key(path)
}
pub fn is_dir(dir: &str) -> bool {
    let d = dir.trim_end_matches('/');
    store().read().keys().any(|k| match k.strip_prefix(d) {
        Some(rest) => rest.starts_with('/'),
        None => false,
    })
}
pub fn remove_file(path: &str) -> Result<()> {
    store().write().remove(path);
    Ok(())
}
pub fn remove_dir(dir: &str, _recursive: bool) -> Result<()> {
    let d = dir.trim_end_matches('/');
    let prefix = format!("{d}/");
    store().write().retain(|k, _| !k.starts_with(&prefix));
    Ok(())
}
pub fn copy(src: &str, dst: &str) -> Result<()> {
    let bytes = read(src)?;
    write(dst, &bytes)
}
pub fn rename(src: &str, dst: &str) -> Result<()> {
    let bytes = read(src)?;
    write(dst, &bytes)?;
    remove_file(src)
}
pub fn create_dir_all(_path: &str) -> Result<()> {
    // Dirs are implicit (prefix of a file key); nothing to allocate.
    Ok(())
}
pub fn modified_ms(_path: &str) -> Result<i64> {
    // No mtime in-memory; report epoch (freshness = oldest) so cache checks
    // treat mounted files as always-stale rather than crashing.
    Ok(0)
}
pub fn file_len(path: &str) -> Result<u64> {
    Ok(read(path)?.len() as u64)
}
/// Immediate child names (direct children only), unsorted.
pub fn read_dir(dir: &str) -> Result<Vec<String>> {
    let d = dir.trim_end_matches('/');
    let mut names: Vec<String> = Vec::new();
    for k in store().read().keys() {
        if let Some(rest) = k.strip_prefix(d) {
            let name = rest.strip_prefix('/').unwrap_or(rest);
            if !name.is_empty() && !name.contains('/') {
                names.push(name.to_string());
            }
        }
    }
    Ok(names)
}
/// Direct children of `dir` whose basename matches `pattern` — full paths, sorted.
pub fn glob(dir: &str, pattern: &str) -> Result<Vec<String>> {
    let d = dir.trim_end_matches('/');
    let mut hits: Vec<String> = Vec::new();
    for k in store().read().keys() {
        if let Some(rest) = k.strip_prefix(d) {
            let name = rest.strip_prefix('/').unwrap_or(rest);
            if !name.is_empty() && !name.contains('/') && super::super::fs::glob_match(pattern, name) {
                hits.push(k.clone());
            }
        }
    }
    hits.sort();
    Ok(hits)
}
/// No fsync in-memory — degrade to a plain write.
pub fn write_atomic(path: &str, bytes: &[u8]) -> Result<()> {
    write(path, bytes)
}

// ── builtins to populate + activate the VFS from z42 / JS ────────────────────

/// `__vfs_mount(path: string, bytes: byte[])` — add a virtual file.
pub fn builtin_vfs_mount(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__vfs_mount: arg 0 must be a path string"),
    };
    let bytes: Vec<u8> = match args.get(1) {
        Some(Value::Array(a)) => a
            .borrow()
            .iter()
            .map(|v| match v {
                Value::I64(n) => (*n & 0xff) as u8,
                _ => 0,
            })
            .collect(),
        _ => bail!("__vfs_mount: arg 1 must be byte[]"),
    };
    store().write().insert(path, bytes);
    Ok(Value::Null)
}

/// `__vfs_enable()` — switch the active fs backend to this in-memory VFS.
pub fn builtin_vfs_enable(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    super::set_backend(super::FsBackend::Memory);
    Ok(Value::Null)
}
