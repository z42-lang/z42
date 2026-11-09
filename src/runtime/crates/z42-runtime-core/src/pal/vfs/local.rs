//! `LocalFs` — the OS file system through `std::fs`. The default VFS on every target
//! with one (`pal::fs::HAS_FS`).
//!
//! Errors stay `std::io::Error`: the builtins (`corelib::fs`) map them to the .NET
//! exception with the path in hand (`fs::fs_error`).
use std::io::Result;
use std::path::Path;

use super::super::fs::{glob_match, sync_before_rename, Vfs};

/// The OS file system. Stateless: every VM on it sees the same disk.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFs;

impl Vfs for LocalFs {
    fn read_to_string(&self, path: &str) -> Result<String> {
        std::fs::read_to_string(path)
    }
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        std::fs::read(path)
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        std::fs::write(path, bytes)
    }
    fn append(&self, path: &str, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).create(true).open(path)?;
        file.write_all(bytes)?;
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }
    fn is_dir(&self, path: &str) -> bool {
        Path::new(path).is_dir()
    }
    fn remove_file(&self, path: &str) -> Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir(&self, path: &str, recursive: bool) -> Result<()> {
        if recursive {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_dir(path)
        }
    }
    fn copy(&self, src: &str, dst: &str) -> Result<()> {
        std::fs::copy(src, dst).map(|_| ())
    }
    fn rename(&self, src: &str, dst: &str) -> Result<()> {
        std::fs::rename(src, dst)
    }
    fn create_dir_all(&self, path: &str) -> Result<()> {
        std::fs::create_dir_all(path)
    }
    fn modified_ms(&self, path: &str) -> Result<i64> {
        use std::time::UNIX_EPOCH;
        let modified = std::fs::metadata(path)?.modified()?;
        Ok(modified
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0))
    }
    fn file_len(&self, path: &str) -> Result<u64> {
        let meta = std::fs::metadata(path)?;
        if meta.is_dir() {
            // .NET `FileInfo.Length` on a directory: FileNotFoundException.
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("'{path}' is a directory")));
        }
        Ok(meta.len())
    }
    fn read_dir(&self, path: &str) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            if let Some(name) = e.file_name().to_str() {
                names.push(name.to_string());
            }
        }
        Ok(names)
    }
    fn glob(&self, dir: &str, pattern: &str) -> Result<Vec<String>> {
        let mut hits: Vec<String> = Vec::new();
        if !Path::new(dir).is_dir() {
            return Ok(hits);
        }
        for entry in std::fs::read_dir(dir)? {
            let e = entry?;
            if let Some(name) = e.file_name().to_str() {
                if glob_match(pattern, name) {
                    let mut full = String::with_capacity(dir.len() + name.len() + 1);
                    full.push_str(dir);
                    if !dir.ends_with('/') {
                        full.push('/');
                    }
                    full.push_str(name);
                    hits.push(full);
                }
            }
        }
        hits.sort();
        Ok(hits)
    }
    /// Tmp sibling + ordered sync + rename (crash-safe): after a crash a reader sees the
    /// old file or the complete new one.
    fn write_atomic(&self, target: &str, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};
        let target_path = Path::new(target);
        let parent = target_path.parent().unwrap_or(Path::new("."));
        let basename = target_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "atomic".to_string());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let tmp = parent.join(format!(".{}.{}.{}.tmp", basename, nanos, pid));
        let result: Result<()> = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            file.write_all(bytes)?;
            sync_before_rename(&file)?;
            drop(file);
            std::fs::rename(&tmp, target_path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
    fn local_path<'a>(&self, path: &'a str) -> Option<&'a Path> {
        Some(Path::new(path))
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod local_tests;
