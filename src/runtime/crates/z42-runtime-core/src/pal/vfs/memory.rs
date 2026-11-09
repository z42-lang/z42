//! `MemoryFs` — an instance-owned `path → bytes` map. The default VFS where the target has
//! no OS file system (wasm); an embedder [`mount`](MemoryFs::mount)s the stdlib / compiler
//! zpkgs into one and hands it to the VMs it creates, after which `DepScan` / z42c /
//! scripting run unchanged.
//!
//! Read / glob / exists are the compile-critical ops (DepScan). Write ops mutate the map;
//! `write_atomic` degrades to a plain write (no fsync in memory). Directory semantics are
//! path-prefix derived: a directory "exists" iff some file lives under it.
use parking_lot::RwLock;
use std::collections::HashMap;
use std::io::{Error, ErrorKind, Result};

use super::super::fs::{glob_match, Vfs};

/// In-memory file system. Each instance is its own store: two VMs handed two instances
/// never see each other's files; VMs sharing one `Arc<MemoryFs>` share it.
#[derive(Debug, Default)]
pub struct MemoryFs {
    files: RwLock<HashMap<String, Vec<u8>>>,
}

impl MemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) a virtual file. The embedder's way to populate the store before
    /// any VM runs on it.
    pub fn mount(&self, path: &str, bytes: Vec<u8>) {
        self.files.write().insert(path.to_string(), bytes);
    }
}

impl Vfs for MemoryFs {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.files
            .read()
            .get(path)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("vfs: `{path}` not mounted")))
    }
    fn read_to_string(&self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| Error::new(ErrorKind::InvalidData, format!("vfs: `{path}` not utf8: {e}")))
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        self.files.write().insert(path.to_string(), bytes.to_vec());
        Ok(())
    }
    fn append(&self, path: &str, bytes: &[u8]) -> Result<()> {
        self.files.write().entry(path.to_string()).or_default().extend_from_slice(bytes);
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        self.files.read().contains_key(path)
    }
    fn is_dir(&self, dir: &str) -> bool {
        let d = dir.trim_end_matches('/');
        self.files.read().keys().any(|k| match k.strip_prefix(d) {
            Some(rest) => rest.starts_with('/'),
            None => false,
        })
    }
    fn remove_file(&self, path: &str) -> Result<()> {
        self.files.write().remove(path);
        Ok(())
    }
    fn remove_dir(&self, dir: &str, _recursive: bool) -> Result<()> {
        let d = dir.trim_end_matches('/');
        let prefix = format!("{d}/");
        self.files.write().retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }
    fn copy(&self, src: &str, dst: &str) -> Result<()> {
        let bytes = self.read(src)?;
        self.write(dst, &bytes)
    }
    fn rename(&self, src: &str, dst: &str) -> Result<()> {
        let bytes = self.read(src)?;
        self.write(dst, &bytes)?;
        self.remove_file(src)
    }
    fn create_dir_all(&self, _path: &str) -> Result<()> {
        // Directories are implicit (a prefix of a file key); nothing to allocate.
        Ok(())
    }
    fn modified_ms(&self, _path: &str) -> Result<i64> {
        // No mtime in memory: report epoch (freshness = oldest), so cache checks treat
        // mounted files as always stale rather than crashing.
        Ok(0)
    }
    fn file_len(&self, path: &str) -> Result<u64> {
        Ok(self.read(path)?.len() as u64)
    }
    fn read_dir(&self, dir: &str) -> Result<Vec<String>> {
        let d = dir.trim_end_matches('/');
        let mut names: Vec<String> = Vec::new();
        for k in self.files.read().keys() {
            if let Some(rest) = k.strip_prefix(d) {
                let name = rest.strip_prefix('/').unwrap_or(rest);
                if !name.is_empty() && !name.contains('/') {
                    names.push(name.to_string());
                }
            }
        }
        Ok(names)
    }
    fn glob(&self, dir: &str, pattern: &str) -> Result<Vec<String>> {
        let d = dir.trim_end_matches('/');
        let mut hits: Vec<String> = Vec::new();
        for k in self.files.read().keys() {
            if let Some(rest) = k.strip_prefix(d) {
                let name = rest.strip_prefix('/').unwrap_or(rest);
                if !name.is_empty() && !name.contains('/') && glob_match(pattern, name) {
                    hits.push(k.clone());
                }
            }
        }
        hits.sort();
        Ok(hits)
    }
    /// No fsync in memory — a plain write is already all-or-nothing under the map lock.
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<()> {
        self.write(path, bytes)
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod memory_tests;
