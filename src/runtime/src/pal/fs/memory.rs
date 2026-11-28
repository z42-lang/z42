//! `MemoryFs` — an instance-owned `path → bytes` map. The default VFS where the target has
//! no OS file system (wasm); an embedder [`mount`](MemoryFs::mount)s the stdlib / compiler
//! zpkgs into one and hands it to the VMs it creates, after which `DepScan` / z42c /
//! scripting run unchanged.
//!
//! Read / glob / exists are the compile-critical ops (DepScan). Write ops mutate the map;
//! `write_atomic` degrades to a plain write (no fsync in memory). Directory semantics are
//! derived from complete path components: a directory exists when a file lives under it.
use parking_lot::RwLock;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::io::{Error, ErrorKind, Result};

use super::{glob_match, Vfs};

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
        self.files.write().insert(file_key(path).into_owned(), bytes);
    }
}

fn file_key(path: &str) -> Cow<'_, str> {
    // Mount paths use `/`. Windows Path::join may produce `\` for the same path.
    if cfg!(windows) && path.contains('\\') {
        return Cow::Owned(path.replace('\\', "/"));
    }
    Cow::Borrowed(path)
}

fn directory_prefix(dir: &str) -> String {
    let mut prefix = file_key(dir).into_owned();
    if !prefix.is_empty() {
        prefix.truncate(prefix.trim_end_matches('/').len());
        prefix.push('/');
    }
    prefix
}

fn descendant<'a>(file: &'a str, prefix: &str) -> Option<&'a str> {
    let path = file.strip_prefix(prefix)?;
    // Keep the empty (relative) directory separate from the absolute root `/`.
    if cfg!(windows) && prefix.is_empty() && std::path::Path::new(path).has_root() {
        return None;
    }
    (!path.is_empty() && !path.starts_with('/')).then_some(path)
}

fn child_names<'a>(files: &'a HashMap<String, Vec<u8>>, prefix: &str) -> BTreeSet<&'a str> {
    files.keys()
        .filter_map(|path| descendant(path, prefix))
        .map(|path| path.split_once('/').map_or(path, |(name, _)| name))
        .collect()
}

fn not_mounted(path: &str) -> Error {
    Error::new(ErrorKind::NotFound, format!("vfs: `{path}` not mounted"))
}

impl Vfs for MemoryFs {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.files
            .read()
            .get(file_key(path).as_ref())
            .cloned()
            .ok_or_else(|| not_mounted(path))
    }
    fn read_to_string(&self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| Error::new(ErrorKind::InvalidData, format!("vfs: `{path}` not utf8: {e}")))
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        self.mount(path, bytes.to_vec());
        Ok(())
    }
    fn append(&self, path: &str, bytes: &[u8]) -> Result<()> {
        self.files.write().entry(file_key(path).into_owned()).or_default().extend_from_slice(bytes);
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        self.files.read().contains_key(file_key(path).as_ref())
    }
    fn is_dir(&self, dir: &str) -> bool {
        let prefix = directory_prefix(dir);
        self.files.read().keys().any(|path| descendant(path, &prefix).is_some())
    }
    fn remove_file(&self, path: &str) -> Result<()> {
        self.files.write().remove(file_key(path).as_ref());
        Ok(())
    }
    fn remove_dir(&self, dir: &str, recursive: bool) -> Result<()> {
        let prefix = directory_prefix(dir);
        let mut files = self.files.write();
        if recursive {
            files.retain(|path, _| descendant(path, &prefix).is_none());
        } else if files.keys().any(|path| descendant(path, &prefix).is_some()) {
            return Err(Error::new(ErrorKind::DirectoryNotEmpty, format!("vfs: `{dir}` is not empty")));
        }
        Ok(())
    }
    fn copy(&self, src: &str, dst: &str) -> Result<()> {
        self.mount(dst, self.read(src)?);
        Ok(())
    }
    fn rename(&self, src: &str, dst: &str) -> Result<()> {
        let mut files = self.files.write();
        let bytes = files.remove(file_key(src).as_ref()).ok_or_else(|| not_mounted(src))?;
        files.insert(file_key(dst).into_owned(), bytes);
        Ok(())
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
        self.files
            .read()
            .get(file_key(path).as_ref())
            .map(|bytes| bytes.len() as u64)
            .ok_or_else(|| not_mounted(path))
    }
    fn read_dir(&self, dir: &str) -> Result<Vec<String>> {
        let prefix = directory_prefix(dir);
        Ok(child_names(&self.files.read(), &prefix).into_iter().map(str::to_owned).collect())
    }
    fn glob(&self, dir: &str, pattern: &str) -> Result<Vec<String>> {
        let prefix = directory_prefix(dir);
        Ok(child_names(&self.files.read(), &prefix).into_iter()
            .filter(|name| glob_match(pattern, name))
            .map(|name| format!("{prefix}{name}"))
            .collect())
    }
    /// No fsync in memory — a plain write is already all-or-nothing under the map lock.
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<()> {
        self.write(path, bytes)
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "memory_tests.rs"]
mod memory_tests;
