//! Filesystem platform operations — permission bits + symlinks.
//!
//! review.md Part 1 P2 Phase 2 (add-pal-fs): migrates the
//! `#[cfg(unix)]` blocks out of `corelib/fs.rs::builtin_file_make_executable`
//! and `builtin_file_symlink`. The unix / non-unix split lives entirely inside
//! this file, so `corelib/fs.rs` carries zero `#[cfg(...)]` for these ops
//! (mirrors the Phase 1 `pal/system.rs` shape).

use std::io::Result;

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
