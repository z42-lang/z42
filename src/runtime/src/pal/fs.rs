//! Filesystem platform operations — permission bits + symlinks.
//!
//! review.md Part 1 P2 Phase 2 (add-pal-fs): migrates the
//! `#[cfg(unix)]` blocks out of `corelib/fs.rs::builtin_file_make_executable`
//! and `builtin_file_symlink`. The unix / non-unix split lives entirely inside
//! this file, so `corelib/fs.rs` carries zero `#[cfg(...)]` for these ops
//! (mirrors the Phase 1 `pal/system.rs` shape).

use std::io::Result;

/// Whether the target has an OS file system (wasm32 runs on the in-memory VFS).
pub const HAS_FS: bool = !cfg!(target_arch = "wasm32");

/// Whether an `InvalidFilename` error means the name was too long (`ENAMETOOLONG`).
/// Platforms without the errno distinction report `true`.
pub fn is_name_too_long(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    return e.raw_os_error() == Some(libc::ENAMETOOLONG);
    #[cfg(not(unix))]
    {
        let _ = e;
        true
    }
}

/// Make a tmp file's contents reach the disk no later than the rename that publishes it —
/// the crash-safety argument of an atomic write: after a crash a reader sees the old file
/// or the complete new one.
///
/// That needs ordering, not a full flush of the drive cache. `File::sync_all` is
/// `F_FULLFSYNC` on Apple (drains the device cache; it was 6–12 % of a z42c build, which
/// writes every compile-cache file through an atomic write) and `fsync` elsewhere (also
/// flushes metadata the rename rewrites anyway).
/// - Apple: `F_BARRIERFSYNC` — writes issued before it hit the media before writes issued
///   after it (the rename). Falls back to `sync_data` if the file system rejects it.
/// - Linux / Android: `fdatasync` (`sync_data`).
/// - Elsewhere: `sync_all`.
pub fn sync_before_rename(file: &std::fs::File) -> Result<()> {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: plain fcntl on a file descriptor `file` owns for the call.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_BARRIERFSYNC) } == 0 {
            return Ok(());
        }
        file.sync_data()
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        file.sync_data()
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
    {
        file.sync_all()
    }
}

/// The separator of a path list (`PATH`-style): `;` on Windows, `:` elsewhere.
pub const PATH_LIST_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// Add execute permission (`u+x,g+x,o+x`) to `path`. No-op on platforms where
/// executability isn't a permission bit — NTFS decides by file extension, not
/// an ACL bit.
pub fn make_executable(path: &str) -> Result<()> {
    make_executable_impl(path)
}

/// Create a symbolic link `dst → src`. Errors on platforms without unprivileged
/// symlink support (Windows symlink needs a privilege — a real impl lands once a
/// Windows CI runner is in the loop).
pub fn symlink(src: &str, dst: &str) -> Result<()> {
    symlink_impl(src, dst)
}

// ── unix impls ───────────────────────────────────────────────────────────────

#[cfg(unix)]
fn make_executable_impl(path: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = perms.mode() | 0o111;
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)
}

#[cfg(unix)]
fn symlink_impl(src: &str, dst: &str) -> Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

// ── non-unix impls (windows / wasm today) ─────────────────────────────────────

#[cfg(not(unix))]
fn make_executable_impl(path: &str) -> Result<()> {
    let _ = path;
    Ok(())
}

#[cfg(not(unix))]
fn symlink_impl(src: &str, dst: &str) -> Result<()> {
    let _ = (src, dst);
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "File.SymLink: not implemented on this platform"))
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod fs_tests;
