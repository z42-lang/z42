//! Filesystem platform operations — the [`Vfs`] trait every path-based file access goes
//! through, plus the OS calls outside it (permission bits, symlinks, ordered sync).
//!
//! **One `Vfs` per VM.** Whoever creates a VM hands it an `Arc<dyn Vfs>` (or takes
//! [`default_vfs`]); the file builtins, the package loader and the app runner all read
//! through that handle, never through a process-wide switch. Two VMs in one process
//! therefore see separate in-memory file systems. Implementations live in
//! [`super::vfs`]: [`LocalFs`](super::vfs::LocalFs) (`std::fs`) and
//! [`MemoryFs`](super::vfs::MemoryFs) (`path → bytes`, what wasm runs on).
//!
//! The OS implementations live under `pal/sys/`, so
//! `corelib/fs.rs` carries zero `#[cfg(...)]` for them.

use std::io::Result;
use std::path::Path;
use std::sync::Arc;

/// Whether the target has an OS file system (wasm32 runs on the in-memory VFS).
pub const HAS_FS: bool = super::sys::fs::HAS_FS;

/// A file system a VM reads and writes through. Paths are plain strings, results plain std
/// types; errors are `std::io::Error` so the builtins map them to .NET exceptions with
/// the path in hand, the same for every implementation.
pub trait Vfs: Send + Sync {
    fn read_to_string(&self, path: &str) -> Result<String>;
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    fn write(&self, path: &str, bytes: &[u8]) -> Result<()>;
    fn append(&self, path: &str, bytes: &[u8]) -> Result<()>;
    fn exists(&self, path: &str) -> bool;
    fn is_dir(&self, path: &str) -> bool;
    fn remove_file(&self, path: &str) -> Result<()>;
    fn remove_dir(&self, path: &str, recursive: bool) -> Result<()>;
    /// Overwrites an existing destination.
    fn copy(&self, src: &str, dst: &str) -> Result<()>;
    fn rename(&self, src: &str, dst: &str) -> Result<()>;
    fn create_dir_all(&self, path: &str) -> Result<()>;
    /// Last write time, unix epoch milliseconds.
    fn modified_ms(&self, path: &str) -> Result<i64>;
    /// File size; a directory is `NotFound` (.NET `FileInfo.Length`).
    fn file_len(&self, path: &str) -> Result<u64>;
    /// Immediate child names (files + subdirectories), unsorted.
    fn read_dir(&self, path: &str) -> Result<Vec<String>>;
    /// Direct children of `dir` whose basename matches `pattern` ([`glob_match`]) — full
    /// paths, sorted.
    fn glob(&self, dir: &str, pattern: &str) -> Result<Vec<String>>;
    /// Write so a reader sees the old contents or the complete new ones, never a mix.
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<()>;

    /// `path` as a file on the OS file system, when this VFS is one. A caller that needs a
    /// property only a real file system has — real directories, real modification times
    /// worth caching on — asks this instead of checking which implementation it holds.
    /// `None` (the default) for a virtual file system.
    fn local_path<'a>(&self, path: &'a str) -> Option<&'a Path> {
        let _ = path;
        None
    }
}

/// The VFS a VM gets when its creator names none: the OS file system where the target has
/// one ([`HAS_FS`]), otherwise a fresh, empty in-memory one.
pub fn default_vfs() -> Arc<dyn Vfs> {
    if HAS_FS {
        Arc::new(super::vfs::LocalFs)
    } else {
        Arc::new(super::vfs::MemoryFs::new())
    }
}

/// `*` / `?` glob matcher — recursion-free, backtracking via two cursors (the classic
/// K&R wildcard match). The pattern language of [`Vfs::glob`].
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star_pi: Option<usize> = None;
    let mut star_ti = 0usize;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' { pi += 1; }
    pi == p.len()
}

/// Whether an `InvalidFilename` error means the name was too long (`ENAMETOOLONG`).
/// Platforms without the errno distinction report `true`.
pub fn is_name_too_long(e: &std::io::Error) -> bool {
    super::sys::fs::is_name_too_long(e)
}

/// Make a tmp file's contents reach the disk no later than the rename that publishes it —
/// the crash-safety argument of an atomic write: after a crash a reader sees the old file
/// or the complete new one.
///
/// That needs ordering, not a full flush of the drive cache. `File::sync_all` is
/// `F_FULLFSYNC` on Apple (drains the device cache) and `fsync` elsewhere (also
/// flushes metadata the rename rewrites anyway).
/// - Apple: `F_BARRIERFSYNC` — writes issued before it hit the media before writes issued
///   after it (the rename). Falls back to `sync_data` if the file system rejects it.
/// - Linux / Android: `fdatasync` (`sync_data`).
/// - Elsewhere: `sync_all`.
pub fn sync_before_rename(file: &std::fs::File) -> Result<()> {
    super::sys::fs::sync_before_rename(file)
}

/// The separator of a path list (`PATH`-style): `;` on Windows, `:` elsewhere.
pub const PATH_LIST_SEP: char = super::sys::fs::PATH_LIST_SEP;

/// Add execute permission (`u+x,g+x,o+x`) to `path`. No-op on platforms where
/// executability isn't a permission bit — NTFS decides by file extension, not
/// an ACL bit.
pub fn make_executable(path: &str) -> Result<()> {
    super::sys::fs::make_executable(path)
}

/// Create a symbolic link `dst → src`. Errors on platforms without unprivileged
/// symlink support; the current non-Unix backends return `Unsupported`.
pub fn symlink(src: &str, dst: &str) -> Result<()> {
    super::sys::fs::symlink(src, dst)
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod fs_tests;
