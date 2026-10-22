//! File system / environment / clock builtins (builtin ABI v2 — the `typed` module).
//!
//! - **Path-based file ops are platform-agnostic** (add-wasm-vfs-backend): they only
//!   call `active().X`; platform differences (native `std::fs` / the wasm in-memory VFS)
//!   live in `fs_backend/{native,memory}.rs`, which return `std::io::Error`.
//! - **Errors are .NET's** ([`fs_error`]): a missing file → `FileNotFoundException`, a
//!   missing directory (or a missing part of the path) → `DirectoryNotFoundException`,
//!   permission denied / a directory where a file is expected → `UnauthorizedAccessException`,
//!   a path too long → `PathTooLongException`, anything else → `IOException` with the OS
//!   message and the path; a null argument → `ArgumentNullException`, an empty path →
//!   `ArgumentException`. All of them are `IOException` or `Exception` subclasses, so
//!   handlers that caught `Exception` still catch them.
//! - **Every call that reads, writes, opens, creates or enumerates runs parked**
//!   (`blocking` / [`Blocking`]): a collection another thread starts does not wait for a
//!   slow disk, a network file system or a FIFO. Path strings and byte payloads are copied
//!   out before parking, results become `Value`s after unparking. The stat-only probes
//!   (`Exists`, `GetSize`, `GetLastWriteTime`, FileStream `Length` / `Position` / `Seek`)
//!   run unparked: a park retires the thread's allocation buffer (`gc::safepoint`), which
//!   costs more than the `stat` itself — a `File.Exists` loop ran 2.6× slower parked.
//! - **FileStream handles** (`__file_open` …): the slot table `ctx.core.file_handles`
//!   holds `Arc<File>`; an operation clones the `Arc` under the table lock, drops the lock,
//!   then does the syscall — no syscall ever runs under the global lock (it used to:
//!   every FileStream read / write in the process serialized on it, and a thread blocked
//!   in a read held it while unparked, stalling every collection).
//!
//! [`Blocking`]: crate::corelib::abi::Blocking

use crate::corelib::abi::BuiltinError;
use crate::corelib::fs_backend::{active, FsBackend};
use std::io::ErrorKind;
use std::sync::Arc;

/// What a path-based operation expected to find — decides what a missing path means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PathRole {
    /// An existing file (read, open for reading, delete, stat, copy / move source).
    File,
    /// A file to create or write (its directory must exist).
    Create,
    /// An existing directory (enumerate, delete, `cd`).
    Dir,
}

/// Map an OS error on `path` to the exception .NET throws for it (`Interop.GetExceptionForIoErrno`).
///
/// | `io::ErrorKind` | exception | message |
/// |---|---|---|
/// | `NotFound`, role `File`, parent directory exists | `FileNotFoundException` | `Could not find file '<path>'.` |
/// | `NotFound`, otherwise | `DirectoryNotFoundException` | `Could not find a part of the path '<path>'.` |
/// | `PermissionDenied`, `IsADirectory` (role `File` / `Create`) | `UnauthorizedAccessException` | `Access to the path '<path>' is denied.` |
/// | `InvalidFilename` (`ENAMETOOLONG`) | `PathTooLongException` | `The path '<path>' is too long, or a component of the specified path is too long.` |
/// | `AlreadyExists` | `IOException` | `The file '<path>' already exists.` |
/// | `InvalidInput` (a NUL in the path …) | `ArgumentException` | the OS message |
/// | `Unsupported` (wasm: no such OS facility) | `NotSupportedException` | the OS message |
/// | anything else | `IOException` | `<OS message> : '<path>'` |
pub(crate) fn fs_error(e: std::io::Error, path: &str, role: PathRole) -> BuiltinError {
    match e.kind() {
        ErrorKind::NotFound => {
            if role == PathRole::File && parent_exists(path) {
                BuiltinError::FileNotFound(format!("Could not find file '{path}'."))
            } else {
                BuiltinError::DirectoryNotFound(format!("Could not find a part of the path '{path}'."))
            }
        }
        ErrorKind::PermissionDenied => denied(path),
        ErrorKind::IsADirectory if role != PathRole::Dir => denied(path),
        ErrorKind::InvalidFilename if is_name_too_long(&e) => BuiltinError::PathTooLong(format!(
            "The path '{path}' is too long, or a component of the specified path is too long.")),
        ErrorKind::AlreadyExists => BuiltinError::Io(format!("The file '{path}' already exists.")),
        ErrorKind::InvalidInput => BuiltinError::Argument(e.to_string()),
        ErrorKind::Unsupported => BuiltinError::NotSupported(e.to_string()),
        _ => BuiltinError::Io(format!("{e} : '{path}'")),
    }
}

fn denied(path: &str) -> BuiltinError {
    BuiltinError::UnauthorizedAccess(format!("Access to the path '{path}' is denied."))
}

#[cfg(unix)]
fn is_name_too_long(e: &std::io::Error) -> bool { e.raw_os_error() == Some(libc::ENAMETOOLONG) }
#[cfg(not(unix))]
fn is_name_too_long(_: &std::io::Error) -> bool { true }

/// Whether `path`'s directory exists — a missing file in an existing directory is
/// `FileNotFound`, a missing directory `DirectoryNotFound` (.NET's distinction). The VFS
/// has no real directories: always "exists" there.
fn parent_exists(path: &str) -> bool {
    match active() {
        FsBackend::Memory => true,
        FsBackend::Native => match std::path::Path::new(path).parent() {
            None => true,
            Some(p) if p.as_os_str().is_empty() => true,
            Some(p) => p.is_dir(),
        },
    }
}

/// .NET rejects an empty path before touching the OS.
fn check_path(path: &str, param: &str) -> Result<(), BuiltinError> {
    if path.is_empty() {
        return Err(BuiltinError::Argument(format!(
            "The value cannot be an empty string. (Parameter '{param}')")));
    }
    Ok(())
}

/// A two-path op (copy / move / link) failed: blame the source when it is missing,
/// otherwise the destination (.NET `File.Copy`: missing source → `FileNotFoundException`,
/// missing destination directory → `DirectoryNotFoundException`).
fn two_path_error(e: std::io::Error, src: &str, dst: &str) -> BuiltinError {
    if active().exists(src) { fs_error(e, dst, PathRole::Create) } else { fs_error(e, src, PathRole::File) }
}

/// Depth-first listing relative to `root`, backend-agnostic (`active().read_dir` +
/// `is_dir`). `rel` = the current directory relative to `root` (`""` = root itself).
fn walk_dir(root: &str, rel: &str, out: &mut Vec<String>) -> Result<(), BuiltinError> {
    let cur = if rel.is_empty() {
        root.trim_end_matches('/').to_string()
    } else {
        format!("{}/{}", root.trim_end_matches('/'), rel)
    };
    let names = active().read_dir(&cur).map_err(|e| fs_error(e, &cur, PathRole::Dir))?;
    for name in names {
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        out.push(child_rel.clone());
        let child_path = format!("{cur}/{name}");
        if active().is_dir(&child_path) {
            walk_dir(root, &child_rel, out)?;
        }
    }
    Ok(())
}

/// `*` / `?` glob matcher — recursion-free, backtracking via two cursors
/// (same idea as the classic K&R wildcard match).
pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
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

fn unique_temp_path(prefix: &str, suffix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // fix-wasm-time-builtins: guard `std::time` (panics on wasm32). The COUNTER +
    // pid already give uniqueness; nanos is just extra entropy → 0 on wasm.
    #[cfg(not(target_arch = "wasm32"))]
    let nanos = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64).unwrap_or(0)
    };
    #[cfg(target_arch = "wasm32")]
    let nanos = 0u64;
    // fix-wasm-corpus-capability-gate: std::process::id() panics on wasm32.
    #[cfg(not(target_arch = "wasm32"))]
    let pid = std::process::id();
    #[cfg(target_arch = "wasm32")]
    let pid = 0u32;
    let bump  = COUNTER.fetch_add(1, Ordering::Relaxed);
    // fix-wasm-corpus-capability-gate: std::env::temp_dir() panics on wasm32
    // (sys/paths/unsupported). "/tmp" in the in-memory VFS works fine.
    #[cfg(not(target_arch = "wasm32"))]
    let mut p = std::env::temp_dir();
    #[cfg(target_arch = "wasm32")]
    let mut p = std::path::PathBuf::from("/tmp");
    p.push(format!("{prefix}.{nanos:x}.{pid}.{bump:x}{suffix}"));
    p.to_string_lossy().into_owned()
}

#[cfg(not(target_arch = "wasm32"))]
fn wall_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64).unwrap_or(0)
}

// fix-wasm-time-builtins: `wasm32-unknown-unknown` has no `std::time`
// (`SystemTime::now()` panics "time not implemented"). Return epoch 0 so
// `Std.Time` doesn't trap in the browser; real wall-clock via JS `Date.now` is
// Deferred (would need js-sys in this crate).
#[cfg(target_arch = "wasm32")]
fn wall_now_ms() -> i64 { 0 }

// ── FileStream slot table ─────────────────────────────────────────────────────
//
// add-z42-io-filestream : each opened OS handle lands in
// `VmCore.file_handles` keyed by a monotonic id; z42-side `Std.IO.FileStream` holds the
// id. P1-8 PR 2: the slot holds an `Arc<File>` — operations clone it out under the table
// lock and do the syscall after dropping the lock (`&File` implements Read / Write /
// Seek). `__file_close` takes the `Arc` out and leaves the slot present, so a later call
// on the id reports "closed"; a read still running on another thread keeps its clone, so
// the descriptor is closed only after that read returns (no fd reuse under it).

/// 0 = Read (open existing, read-only)
/// 1 = Write (create-or-truncate, write-only)
/// 2 = Append (create-if-missing / append, write-only; O_APPEND on POSIX
///     so writes always go to EOF and `seek` is a no-op on the write
///     side — z42 facade reports CanSeek() == false in this mode)
pub(crate) struct FileHandleSlot {
    pub(crate) file: Option<Arc<std::fs::File>>,
    #[allow(dead_code)] // reserved for future stricter mode-aware error messages
    pub(crate) mode: i64,
}

/// The open file behind `slot` (a clone of its `Arc`; the table lock is released on return).
/// An unknown or closed slot is .NET's `ObjectDisposedException` ("Cannot access a closed
/// file.", `ObjectName` null).
fn open_file(ctx: &crate::vm_context::VmContext, slot: i64) -> Result<Arc<std::fs::File>, BuiltinError> {
    ctx.core.file_handles.with_mut(slot as u64, |s| s.file.clone()).flatten()
        .ok_or_else(|| BuiltinError::object_disposed(None, "Cannot access a closed file."))
}

/// .NET `Stream.Read` / `Write` argument checks against a buffer of `len` bytes.
pub(crate) fn check_buffer_range(offset: i64, count: i64, len: usize) -> Result<(usize, usize), BuiltinError> {
    if offset < 0 {
        return Err(BuiltinError::ArgumentOutOfRange("Non-negative number required. (Parameter 'offset')".into()));
    }
    if count < 0 {
        return Err(BuiltinError::ArgumentOutOfRange("Non-negative number required. (Parameter 'count')".into()));
    }
    let (off, n) = (offset as usize, count as usize);
    if off.checked_add(n).is_none_or(|end| end > len) {
        return Err(BuiltinError::Argument(
            "Offset and length were out of bounds for the array or count is greater than the number of \
             elements from index to the end of the source collection.".into()));
    }
    Ok((off, n))
}

pub(crate) fn byte_array(v: &crate::metadata::Value, param: &str) -> Result<crate::gc::ArrRef, BuiltinError> {
    match v {
        crate::metadata::Value::Array(rc) => Ok(*rc),
        crate::metadata::Value::Null => Err(BuiltinError::ArgumentNull(format!("Value cannot be null. (Parameter '{param}')"))),
        other => Err(BuiltinError::InvalidCast(format!(
            "argument `{param}` expects a byte[], got {}", crate::semantics::value_kind_name(other)))),
    }
}

/// `buf[off..off + n]` copied out (packed `byte[]`: one atomic chunked copy).
pub(crate) fn read_byte_range(arr: &crate::metadata::types::ArrayObj, off: usize, n: usize) -> Result<Vec<u8>, BuiltinError> {
    if let Some(b) = arr.byte_range(off, n) { return Ok(b); }
    (off..off + n).map(|i| match arr.get_boxed(i) {
        crate::metadata::Value::I64(b) if (0..=255).contains(&b) => Ok(b as u8),
        other => Err(BuiltinError::InvalidCast(format!(
            "buffer element {i} is not a byte: {}", crate::semantics::value_kind_name(&other)))),
    }).collect()
}

/// Store `bytes` into `buf` from `off` on.
pub(crate) fn write_byte_range(arr: &crate::metadata::types::ArrayObj, off: usize, bytes: &[u8]) {
    if !arr.write_byte_range(off, bytes) {
        for (i, b) in bytes.iter().enumerate() {
            arr.set_boxed(off + i, crate::metadata::Value::I64(*b as i64));
        }
    }
}

#[z42_macros::builtins]
pub mod typed {
    use super::{check_buffer_range, check_path, fs_error, open_file, two_path_error, FileHandleSlot, PathRole};
    use crate::corelib::abi::{Blocking, BuiltinError, Then};
    use crate::corelib::fs_backend::active;
    use crate::metadata::Value;
    use crate::vm_context::VmContext;
    use std::io::{Read, Seek, SeekFrom, Write};

    // ── File (path-based) ─────────────────────────────────────────────────────

    #[builtin(name = "__file_read_text", blocking)]
    pub fn file_read_text(path: String) -> Result<String, BuiltinError> {
        check_path(&path, "path")?;
        active().read_to_string(&path).map_err(|e| fs_error(e, &path, PathRole::File))
    }

    #[builtin(name = "__file_write_text", blocking)]
    pub fn file_write_text(path: String, contents: String) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().write(&path, contents.as_bytes()).map_err(|e| fs_error(e, &path, PathRole::Create))
    }

    #[builtin(name = "__file_append_text", blocking)]
    pub fn file_append_text(path: String, contents: String) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().append(&path, contents.as_bytes()).map_err(|e| fs_error(e, &path, PathRole::Create))
    }

    /// .NET `File.Exists`: false for a null / empty path, never throws.
    #[builtin(name = "__file_exists")]
    pub fn file_exists(path: Option<&str>) -> bool {
        path.is_some_and(|p| !p.is_empty() && active().exists(p))
    }

    /// Deleting a missing file throws `FileNotFoundException` (z42 keeps throwing where
    /// .NET's `File.Delete` is silent — a script that deletes the wrong path should hear
    /// about it); a missing directory `DirectoryNotFoundException` as in .NET.
    #[builtin(name = "__file_delete", blocking)]
    pub fn file_delete(path: String) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().remove_file(&path).map_err(|e| fs_error(e, &path, PathRole::File))
    }

    /// Overwrites an existing destination (z42 semantics; .NET's two-argument `Copy` refuses).
    #[builtin(name = "__file_copy", blocking)]
    pub fn file_copy(src: String, dst: String) -> Result<(), BuiltinError> {
        check_path(&src, "src")?;
        check_path(&dst, "dst")?;
        active().copy(&src, &dst).map_err(|e| two_path_error(e, &src, &dst))
    }

    #[builtin(name = "__file_move", blocking)]
    pub fn file_move(src: String, dst: String) -> Result<(), BuiltinError> {
        check_path(&src, "src")?;
        check_path(&dst, "dst")?;
        active().rename(&src, &dst).map_err(|e| two_path_error(e, &src, &dst))
    }

    /// add-file-last-write-time : mtime as unix epoch milliseconds (same unit as
    /// `__time_now_ms`; `File.GetLastWriteTime` wraps it into a `DateTime`).
    #[builtin(name = "__file_last_write_time_ms")]
    pub fn file_last_write_time_ms(path: &str) -> Result<i64, BuiltinError> {
        check_path(path, "path")?;
        active().modified_ms(path).map_err(|e| fs_error(e, path, PathRole::File))
    }

    /// add-z42-io-ergonomics-bytes-glob : one-shot binary read → packed `byte[]`.
    #[builtin(name = "__file_read_bytes", blocking)]
    pub fn file_read_bytes(path: String) -> Result<Vec<u8>, BuiltinError> {
        check_path(&path, "path")?;
        active().read(&path).map_err(|e| fs_error(e, &path, PathRole::File))
    }

    #[builtin(name = "__file_write_bytes", blocking)]
    pub fn file_write_bytes(path: String, bytes: Vec<u8>) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().write(&path, &bytes).map_err(|e| fs_error(e, &path, PathRole::Create))
    }

    /// add-file-atomic-write : tmp sibling + ordered sync
    /// (`F_BARRIERFSYNC` / `fdatasync`) + rename — the whole sequence runs parked.
    #[builtin(name = "__file_write_text_atomic", blocking)]
    pub fn file_write_text_atomic(path: String, contents: String) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().write_atomic(&path, contents.as_bytes()).map_err(|e| fs_error(e, &path, PathRole::Create))
    }

    #[builtin(name = "__file_write_bytes_atomic", blocking)]
    pub fn file_write_bytes_atomic(path: String, bytes: Vec<u8>) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().write_atomic(&path, &bytes).map_err(|e| fs_error(e, &path, PathRole::Create))
    }

    /// extend-z42-io-glob-temp : a unique directory under the system temp root.
    #[builtin(name = "__file_create_temp_dir", blocking)]
    pub fn file_create_temp_dir(prefix: String) -> Result<String, BuiltinError> {
        let path = super::unique_temp_path(&prefix, "");
        std::fs::create_dir_all(&path).map_err(|e| fs_error(e, &path, PathRole::Create))?;
        Ok(path)
    }

    /// A unique empty file under the system temp root (`suffix` may be "").
    #[builtin(name = "__file_create_temp_file", blocking)]
    pub fn file_create_temp_file(prefix: String, suffix: String) -> Result<String, BuiltinError> {
        let path = super::unique_temp_path(&prefix, &suffix);
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&path)
            .map_err(|e| fs_error(e, &path, PathRole::Create))?;
        Ok(path)
    }

    /// Unix: `chmod u+x,g+x,o+x`; Windows: no-op (NTFS decides by extension).
    #[builtin(name = "__file_make_executable", blocking)]
    pub fn file_make_executable(path: String) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        crate::pal::fs::make_executable(&path).map_err(|e| fs_error(e, &path, PathRole::File))
    }

    /// Hard link `dst → src`.
    #[builtin(name = "__file_link", blocking)]
    pub fn file_link(src: String, dst: String) -> Result<(), BuiltinError> {
        check_path(&src, "src")?;
        check_path(&dst, "dst")?;
        std::fs::hard_link(&src, &dst).map_err(|e| two_path_error(e, &src, &dst))
    }

    /// Symbolic link `dst → src` (the target need not exist).
    #[builtin(name = "__file_symlink", blocking)]
    pub fn file_symlink(src: String, dst: String) -> Result<(), BuiltinError> {
        check_path(&src, "src")?;
        check_path(&dst, "dst")?;
        crate::pal::fs::symlink(&src, &dst).map_err(|e| fs_error(e, &dst, PathRole::Create))
    }

    /// File size in bytes; a directory is `FileNotFoundException` (.NET `FileInfo.Length`).
    #[builtin(name = "__file_get_size")]
    pub fn file_get_size(path: &str) -> Result<i64, BuiltinError> {
        check_path(path, "path")?;
        active().file_len(path).map(|n| n as i64).map_err(|e| fs_error(e, path, PathRole::File))
    }

    /// Glob `dir/pattern` direct children (`*` / `?`; no `/` in the pattern) — sorted full paths.
    #[builtin(name = "__path_glob", blocking)]
    pub fn path_glob(dir: String, pattern: String) -> Result<Vec<String>, BuiltinError> {
        active().glob(&dir, &pattern).map_err(|e| fs_error(e, &dir, PathRole::Dir))
    }

    // ── Directory ─────────────────────────────────────────────────────────────

    /// .NET `Directory.Exists`: false for a null / empty path, never throws.
    #[builtin(name = "__dir_exists")]
    pub fn dir_exists(path: Option<&str>) -> bool {
        path.is_some_and(|p| !p.is_empty() && active().is_dir(p))
    }

    /// `mkdir -p`; an existing file at `path` is `IOException` (.NET `CreateDirectory`).
    #[builtin(name = "__dir_create", blocking)]
    pub fn dir_create(path: String) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().create_dir_all(&path).map_err(|e| fs_error(e, &path, PathRole::Create))
    }

    /// Missing → `DirectoryNotFoundException`; not empty (non-recursive) → `IOException`.
    #[builtin(name = "__dir_delete", blocking)]
    pub fn dir_delete(path: String, recursive: bool) -> Result<(), BuiltinError> {
        check_path(&path, "path")?;
        active().remove_dir(&path, recursive).map_err(|e| fs_error(e, &path, PathRole::Dir))
    }

    /// Direct children (basenames), sorted.
    #[builtin(name = "__dir_enumerate", blocking)]
    pub fn dir_enumerate(path: String) -> Result<Vec<String>, BuiltinError> {
        check_path(&path, "path")?;
        let mut names = active().read_dir(&path).map_err(|e| fs_error(e, &path, PathRole::Dir))?;
        names.sort();
        Ok(names)
    }

    /// Every descendant, as paths relative to `path`, sorted.
    #[builtin(name = "__dir_enumerate_recursive", blocking)]
    pub fn dir_enumerate_recursive(path: String) -> Result<Vec<String>, BuiltinError> {
        check_path(&path, "path")?;
        let mut out = Vec::new();
        super::walk_dir(&path, "", &mut out)?;
        out.sort();
        Ok(out)
    }

    // ── FileStream handles ────────────────────────────────────────────────────

    /// `__file_open(path, mode) -> long`. The open runs parked; the handle is registered
    /// after unparking. Reading a directory is `UnauthorizedAccessException` (.NET).
    #[builtin(name = "__file_open")]
    pub fn file_open(path: String, mode: i64) -> Result<Then<i64>, BuiltinError> {
        check_path(&path, "path")?;
        if !(0..=2).contains(&mode) {
            return Err(BuiltinError::ArgumentOutOfRange(format!(
                "Enum value {mode} was out of legal range (0 = Read, 1 = Write, 2 = Append). (Parameter 'mode')")));
        }
        Ok(Blocking::new(move || {
            let mut o = std::fs::OpenOptions::new();
            match mode {
                0 => o.read(true),
                1 => o.write(true).create(true).truncate(true),
                _ => o.append(true).create(true),
            };
            let role = if mode == 0 { PathRole::File } else { PathRole::Create };
            let file = o.open(&path).map_err(|e| fs_error(e, &path, role))?;
            // Opening a directory for writing fails in the OS already; for reading it succeeds.
            if mode == 0 && file.metadata().is_ok_and(|m| m.is_dir()) {
                return Err(super::denied(&path));
            }
            Ok(file)
        }).then(move |ctx: &VmContext, file| {
            let id = ctx.core.file_handles.insert_new(FileHandleSlot { file: Some(std::sync::Arc::new(file)), mode });
            Ok(id as i64)
        }))
    }

    /// `__file_read(slot, buf, offset, count) -> int` — up to `count` bytes into
    /// `buf[offset..]`; 0 = EOF. Reads parked into a scratch buffer, copies into the
    /// `byte[]` after unparking.
    #[builtin(name = "__file_read")]
    pub fn file_read(ctx: &VmContext, slot: i64, buffer: Value, offset: i64, count: i64) -> Result<Then<i64>, BuiltinError> {
        let arr = super::byte_array(&buffer, "buffer")?;
        let (off, n) = check_buffer_range(offset, count, arr.borrow().len())?;
        let file = open_file(ctx, slot)?;
        let read = if n == 0 {
            Blocking::ready(Vec::new())
        } else {
            Blocking::new(move || {
                let mut tmp = vec![0u8; n];
                let got = (&*file).read(&mut tmp).map_err(BuiltinError::from)?;
                tmp.truncate(got);
                Ok(tmp)
            })
        };
        Ok(read.then(move |_ctx: &VmContext, bytes: Vec<u8>| {
            super::write_byte_range(&arr.borrow(), off, &bytes);
            Ok(bytes.len() as i64)
        }))
    }

    /// `__file_write(slot, buf, offset, count)` — the range is copied out, written parked.
    #[builtin(name = "__file_write")]
    pub fn file_write(ctx: &VmContext, slot: i64, buffer: &Value, offset: i64, count: i64) -> Result<Blocking<()>, BuiltinError> {
        let arr = super::byte_array(buffer, "buffer")?;
        let bytes = {
            let a = arr.borrow();
            let (off, n) = check_buffer_range(offset, count, a.len())?;
            super::read_byte_range(&a, off, n)?
        };
        let file = open_file(ctx, slot)?;
        if bytes.is_empty() {
            return Ok(Blocking::ready(()));
        }
        Ok(Blocking::new(move || (&*file).write_all(&bytes).map_err(BuiltinError::from)))
    }

    /// `__file_seek(slot, offset, origin) -> long` (new absolute position). `lseek` does not
    /// block: runs unparked, outside the table lock.
    #[builtin(name = "__file_seek")]
    pub fn file_seek(ctx: &VmContext, slot: i64, offset: i64, origin: i64) -> Result<i64, BuiltinError> {
        let from = match origin {
            0 => SeekFrom::Start(offset.max(0) as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            n => return Err(BuiltinError::ArgumentOutOfRange(format!(
                "Enum value {n} was out of legal range (0 = Begin, 1 = Current, 2 = End). (Parameter 'origin')"))),
        };
        let file = open_file(ctx, slot)?;
        (&*file).seek(from).map(|p| p as i64).map_err(BuiltinError::from)
    }

    #[builtin(name = "__file_length")]
    pub fn file_length(ctx: &VmContext, slot: i64) -> Result<i64, BuiltinError> {
        let file = open_file(ctx, slot)?;
        file.metadata().map(|m| m.len() as i64).map_err(BuiltinError::from)
    }

    #[builtin(name = "__file_position")]
    pub fn file_position(ctx: &VmContext, slot: i64) -> Result<i64, BuiltinError> {
        let file = open_file(ctx, slot)?;
        (&*file).stream_position().map(|p| p as i64).map_err(BuiltinError::from)
    }

    /// `std::fs::File` has no user-space buffer (`flush` is a no-op today); parked anyway,
    /// so a buffered backend later cannot block a collection.
    #[builtin(name = "__file_flush")]
    pub fn file_flush(ctx: &VmContext, slot: i64) -> Result<Blocking<()>, BuiltinError> {
        let file = open_file(ctx, slot)?;
        Ok(Blocking::new(move || (&*file).flush().map_err(BuiltinError::from)))
    }

    /// `__file_close(slot)` — idempotent; an unknown id is a no-op. The `Arc` is taken out
    /// under the lock and dropped after it (closing the descriptor never runs under it).
    #[builtin(name = "__file_close")]
    pub fn file_close(ctx: &VmContext, slot: i64) {
        let file = ctx.core.file_handles.with_mut(slot as u64, |s| s.file.take()).flatten();
        drop(file);
    }

    // ── Console ───────────────────────────────────────────────────────────────

    /// Whether stdout is a terminal (colour / progress decisions).
    #[builtin(name = "__console_is_terminal")]
    pub fn console_is_terminal() -> bool {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal()
    }

    #[builtin(name = "__console_error_is_terminal")]
    pub fn console_error_is_terminal() -> bool {
        use std::io::IsTerminal;
        std::io::stderr().is_terminal()
    }

    // ── Environment / process / clock ─────────────────────────────────────────

    /// `pwd`. fix-wasm-corpus-capability-gate: `current_dir()` panics on wasm32 — a
    /// browser has no working directory, report the VFS root.
    #[builtin(name = "__env_get_cwd")]
    pub fn env_get_cwd() -> Result<String, BuiltinError> {
        #[cfg(not(target_arch = "wasm32"))]
        return std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).map_err(BuiltinError::from);
        #[cfg(target_arch = "wasm32")]
        return Ok("/".to_string());
    }

    /// `cd path` — missing → `DirectoryNotFoundException` (.NET `SetCurrentDirectory`).
    #[builtin(name = "__env_set_cwd")]
    pub fn env_set_cwd(path: &str) -> Result<(), BuiltinError> {
        check_path(path, "path")?;
        // wasm has no working directory — `cd` is a no-op (see env_get_cwd).
        #[cfg(not(target_arch = "wasm32"))]
        std::env::set_current_dir(path).map_err(|e| fs_error(e, path, PathRole::Dir))?;
        Ok(())
    }

    #[builtin(name = "__env_get")]
    pub fn env_get(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// A null value removes the variable (.NET `SetEnvironmentVariable(name, null)`).
    /// fix-wasm-corpus-capability-gate: `set_var` / `remove_var` panic on wasm32 (a
    /// browser has no OS environment) — no-op there.
    #[builtin(name = "__env_set")]
    pub fn env_set(name: &str, value: Option<&str>) {
        #[cfg(not(target_arch = "wasm32"))]
        // Safety: environment writes race only with other threads' reads; z42 makes no
        // stronger promise than .NET here.
        unsafe {
            match value {
                None => std::env::remove_var(name),
                Some(v) => std::env::set_var(name, v),
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (name, value);
    }

    #[builtin(name = "__env_unset")]
    pub fn env_unset(name: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        // Safety: see `env_set`.
        unsafe { std::env::remove_var(name) }
        #[cfg(target_arch = "wasm32")]
        let _ = name;
    }

    /// `KEY=VALUE` entries.
    #[builtin(name = "__env_vars")]
    pub fn env_vars() -> Vec<String> {
        std::env::vars().map(|(k, v)| format!("{k}={v}")).collect()
    }

    /// add-z42-launcher : the program arguments forwarded after `--` on the
    /// z42vm command line (stored on `VmCore`), not the VM's own argv.
    #[builtin(name = "__env_args")]
    pub fn env_args(ctx: &VmContext) -> Vec<String> {
        ctx.program_args()
    }

    /// `process::exit` runs no destructors: report before leaving (`app::report_on_exit`).
    #[builtin(name = "__process_exit")]
    pub fn process_exit(ctx: &VmContext, code: i64) {
        crate::app::report_on_exit(ctx);
        std::process::exit(code as i32);
    }

    #[builtin(name = "__time_now_ms")]
    pub fn time_now_ms() -> i64 {
        super::wall_now_ms()
    }
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod fs_tests;
