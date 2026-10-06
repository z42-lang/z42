//! Native filesystem backend — `std::fs`. The default on all non-wasm targets.
//! Semantics byte-identical to the pre-refactor inline `std::fs` calls (this file
//! is a straight extraction), so native behaviour is unchanged.
use anyhow::Result;

pub fn read_to_string(path: &str) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}
pub fn read(path: &str) -> Result<Vec<u8>> {
    Ok(std::fs::read(path)?)
}
pub fn write(path: &str, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)?;
    Ok(())
}
pub fn append(path: &str, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).create(true).open(path)?;
    file.write_all(bytes)?;
    Ok(())
}
pub fn exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}
pub fn is_dir(path: &str) -> bool {
    std::path::Path::new(path).is_dir()
}
pub fn remove_file(path: &str) -> Result<()> {
    std::fs::remove_file(path)?;
    Ok(())
}
pub fn remove_dir(path: &str, recursive: bool) -> Result<()> {
    if recursive {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_dir(path)?;
    }
    Ok(())
}
pub fn copy(src: &str, dst: &str) -> Result<()> {
    std::fs::copy(src, dst)?;
    Ok(())
}
pub fn rename(src: &str, dst: &str) -> Result<()> {
    std::fs::rename(src, dst)?;
    Ok(())
}
pub fn create_dir_all(path: &str) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}
pub fn modified_ms(path: &str) -> Result<i64> {
    use std::time::UNIX_EPOCH;
    let modified = std::fs::metadata(path)?.modified()?;
    Ok(modified
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0))
}
pub fn file_len(path: &str) -> Result<u64> {
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        anyhow::bail!("File.GetSize: '{}' is a directory", path);
    }
    Ok(meta.len())
}
/// Immediate child names (files + subdirs), unsorted.
pub fn read_dir(path: &str) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let e = entry?;
        if let Some(name) = e.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    Ok(names)
}
/// Direct children of `dir` whose basename matches `pattern` — full paths, sorted.
pub fn glob(dir: &str, pattern: &str) -> Result<Vec<String>> {
    let mut hits: Vec<String> = Vec::new();
    if !std::path::Path::new(dir).is_dir() {
        return Ok(hits);
    }
    for entry in std::fs::read_dir(dir)? {
        let e = entry?;
        if let Some(name) = e.file_name().to_str() {
            if super::super::fs::glob_match(pattern, name) {
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
/// Make the tmp file's contents reach the disk no later than the rename that
/// publishes it — the whole crash-safety argument of [`write_atomic`]: after a
/// crash a reader sees the old file or the complete new one.
///
/// That needs ordering, not a full flush of the drive cache. `File::sync_all`
/// is `F_FULLFSYNC` on Apple (drains the device cache; it was 6–12 % of a
/// z42c build, which writes every compile-cache file through here) and
/// `fsync` elsewhere (also flushes metadata the rename rewrites anyway).
/// - Apple: `F_BARRIERFSYNC` — writes issued before it hit the media before
///   writes issued after it (the rename). Falls back to `sync_data` if the
///   file system rejects the command.
/// - Linux / Android: `fdatasync` (`sync_data`).
/// - Elsewhere: `sync_all`.
fn sync_before_rename(file: &std::fs::File) -> std::io::Result<()> {
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

/// Atomic write — tmp sibling + ordered sync + rename (crash-safe). Native-only guarantee.
pub fn write_atomic(target: &str, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    let target_path = std::path::Path::new(target);
    let parent = target_path.parent().unwrap_or(std::path::Path::new("."));
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
