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
/// Atomic write — tmp sibling + fsync + rename (crash-safe). Native-only guarantee.
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
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, target_path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
