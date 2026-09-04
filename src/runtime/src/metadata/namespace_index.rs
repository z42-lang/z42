//! Stateless namespace→zpkg index primitive (2026-08-25,
//! refactor-metadata-namespace-index; runtime_review #6 step 2).
//!
//! Both the compile-time resolver (`loader::resolve_namespace`) and the runtime
//! lazy loader (`lazy_loader`) need to answer "which zpkg(s) declare namespace
//! X?" by scanning the search dirs and reading each artifact's NSPC section.
//! That scan is the shared **parse** concern; it is stateless and returns owned
//! [`ZpkgCandidate`]s that the caller either drops after matching (loader —
//! transient) or retains in `declared_zpkgs` for on-demand load (lazy_loader).
//!
//! The candidate carries NO lifecycle state — *when* to load / *what* is already
//! loaded / *when* to release lives entirely in `lazy_loader`. This module is
//! the boundary: parse here, lifecycle there.
//!
//! common-pitfalls.md §1: filenames within a dir are sorted before reading, so
//! the candidate order is deterministic across platforms (read_dir order is not).

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::corelib::fs_backend;
use super::formats::{ZBC_MAGIC, ZPKG_MAGIC};
use super::zbc_reader::read_zpkg_meta;

/// A declared-but-not-yet-loaded zpkg candidate: absolute file path + the
/// namespaces it exports (from its NSPC section). Owned — the caller decides
/// drop-vs-retain. Records exactly the information needed to route a `Call` /
/// `ObjNew` miss to the right zpkg for loading, with no lifecycle state.
#[derive(Debug, Clone)]
pub struct ZpkgCandidate {
    /// Absolute path to the zpkg file.
    pub file_path: PathBuf,
    /// Namespaces exported by this artifact (from its NSPC section).
    pub namespaces: Vec<String>,
}

impl ZpkgCandidate {
    /// Build a candidate by reading the zpkg metadata from disk.
    ///
    /// add-wasm-testhost (G6): reads via the platform fs backend so a wasm host
    /// that mounts stdlib zpkgs into the in-memory VFS resolves declared
    /// candidates too. Byte-identical to `std::fs` on native.
    pub fn build(libs_dir: &Path, file_name: &str) -> Result<Self> {
        let file_path = libs_dir.join(file_name);
        let path_str = file_path.to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF8 zpkg path `{}`", file_path.display()))?;
        let data = fs_backend::active().read(path_str)?;
        let meta = read_zpkg_meta(&data)?;
        Ok(Self {
            file_path,
            namespaces: meta.namespaces,
        })
    }

    /// Build a candidate by searching `dirs` in order for `file_name`, using
    /// the first directory that actually contains the file. Enables colocated
    /// dependency resolution (support-colocated-zpkg-deps, 2026-06-20): an
    /// apphost whose payload + its deps live together (e.g.
    /// `programs/z42c/z42c.driver.zpkg` next to `z42c.core.zpkg`) resolves
    /// those siblings even though they aren't in the stdlib `libs/` dir.
    pub fn build_in_dirs(dirs: &[PathBuf], file_name: &str) -> Result<Self> {
        for dir in dirs {
            let file_path = dir.join(file_name);
            if backend_is_file(&file_path) {
                return Self::build(dir, file_name);
            }
        }
        anyhow::bail!("zpkg `{file_name}` not found in any search dir ({} candidates)", dirs.len())
    }
}

// add-wasm-testhost (G6): `is_file` semantics via the platform fs backend —
// exists && !is_dir. Native delegates to std::fs (byte-identical); on wasm a
// mounted zpkg exists in the VFS and is not a directory.
fn backend_is_file(path: &Path) -> bool {
    match path.to_str() {
        Some(s) => {
            let b = fs_backend::active();
            b.exists(s) && !b.is_dir(s)
        }
        None => false,
    }
}

/// Scan `dirs` (in caller order) for `.zpkg` files, reading each one's NSPC
/// namespace list. Unreadable / non-zpkg / bad-magic files are skipped.
///
/// Returns owned candidates; the caller filters by its own match policy
/// (exact for `resolve_namespace`, prefix for `candidates_for_namespace`) and
/// keeps or drops the result. Filenames within a dir are sorted first
/// (common-pitfalls §1) so the output order is deterministic.
pub fn scan_zpkg_candidates(dirs: &[PathBuf]) -> Vec<ZpkgCandidate> {
    let backend = fs_backend::active();
    let mut out = Vec::new();
    for dir in dirs {
        let dir_str = match dir.to_str() { Some(s) => s, None => continue };
        let mut names = match backend.read_dir(dir_str) { Ok(e) => e, Err(_) => continue };
        names.sort();
        for name in names {
            let path = dir.join(&name);
            if path.extension().and_then(|e| e.to_str()) != Some("zpkg") { continue; }
            let path_str = match path.to_str() { Some(s) => s, None => continue };
            let namespaces = match cached_zpkg_namespaces(path_str) { Some(v) => v, None => continue };
            out.push(ZpkgCandidate { file_path: path, namespaces });
        }
    }
    out
}

/// 每文件的 NSPC 记忆化，按 **(size, mtime_ms)** 校验。
///
/// cache-zpkg-candidate-scan（2026-09-04）：本扫描会把 libs 目录下**每个** zpkg 读进内存
/// 再解析 NSPC，而 `loader::resolve_namespace` 每次调用都做一次完整扫描、
/// `app::build_declared_candidates` 又按入口的**每个** import 命名空间各调一次
/// —— hello world 因此做了 75 次读+解析（3 个命名空间 × 25 个包），实测 5.8–10 ms，
/// 是启动耗时的最大头。单个包的读+解析只要 0.01–0.11 ms，成本全在重复。
///
/// **必须带 (size, mtime) 守卫**：REPL / z42b 会在运行中往 libs 目录写入新编译的 zpkg，
/// 纯 path 键的缓存会读到旧导出面 —— `DepScanCache` 正是这么踩过一次
/// （见 `GeneratorLoader.z42` 的注释）。守卫后重复扫描退化为 readdir + 每文件一次 stat。
fn cached_zpkg_namespaces(path_str: &str) -> Option<Vec<String>> {
    use std::sync::OnceLock;
    type Memo = parking_lot::Mutex<std::collections::HashMap<String, (u64, i64, Vec<String>)>>;
    static MEMO: OnceLock<Memo> = OnceLock::new();

    let backend = fs_backend::active();
    // **只在有真实 mtime 的后端上缓存**。内存后端（wasm host 把 stdlib zpkg 挂进 VFS）
    // 的 `modified_ms` 恒返回 0——它的本意是「按最旧处理 ⇒ 缓存一律视为过期」，
    // 但等值守卫会把它变成「永远命中」，正好相反。所以这里按后端种类显式放行：
    // 新增后端时这个 match 会编译报错，逼着做同样的判断。
    let stamp = match backend {
        fs_backend::FsBackend::Native => {
            match (backend.file_len(path_str), backend.modified_ms(path_str)) {
                (Ok(len), Ok(ms)) => Some((len, ms)),
                _ => None,   // stat 失败 → 退回「每次读+解析」，不缓存
            }
        }
        fs_backend::FsBackend::Memory => None,
    };
    if let Some((len, ms)) = stamp {
        let memo = MEMO.get_or_init(Default::default);
        if let Some((clen, cms, ns)) = memo.lock().get(path_str) {
            if *clen == len && *cms == ms {
                return Some(ns.clone());
            }
        }
    }

    let data = backend.read(path_str).ok()?;
    if data.len() < 4 || &data[0..4] != ZPKG_MAGIC { return None; }
    let namespaces = read_zpkg_namespaces(&data).ok()?;
    if let Some((len, ms)) = stamp {
        MEMO.get_or_init(Default::default)
            .lock()
            .insert(path_str.to_string(), (len, ms, namespaces.clone()));
    }
    Some(namespaces)
}

/// Scan `dirs` (in caller order) for `.zbc` files, reading each one's single
/// declared namespace (NSPC fast-path). A zbc declares at most one namespace;
/// it is represented as a 0/1-element `namespaces` vec for a uniform candidate
/// shape with [`scan_zpkg_candidates`]. Same skip + deterministic-sort rules.
pub fn scan_zbc_candidates(dirs: &[PathBuf]) -> Vec<ZpkgCandidate> {
    let backend = fs_backend::active();
    let mut out = Vec::new();
    for dir in dirs {
        let dir_str = match dir.to_str() { Some(s) => s, None => continue };
        let mut names = match backend.read_dir(dir_str) { Ok(e) => e, Err(_) => continue };
        names.sort();
        for name in names {
            let path = dir.join(&name);
            if path.extension().and_then(|e| e.to_str()) != Some("zbc") { continue; }
            let path_str = match path.to_str() { Some(s) => s, None => continue };
            let data = match backend.read(path_str) { Ok(d) => d, Err(_) => continue };
            if data.len() < 4 || &data[0..4] != ZBC_MAGIC { continue; }
            let file_ns = match read_zbc_namespace(&data) { Ok(n) => n, Err(_) => continue };
            let namespaces = if file_ns.is_empty() { Vec::new() } else { vec![file_ns] };
            out.push(ZpkgCandidate { file_path: path, namespaces });
        }
    }
    out
}

/// Read only the namespaces from a binary zpkg (NSPC section). Thin wrapper over
/// the zbc_reader byte-parse — kept here so the scanners have a single parse
/// entry point alongside [`read_zbc_namespace`].
fn read_zpkg_namespaces(data: &[u8]) -> Result<Vec<String>> {
    super::zbc_reader::read_zpkg_namespaces(data)
}

/// Reads the (single) namespace from a binary zbc buffer (NSPC section
/// fast-path). Returns an empty string when the artifact has no NSPC section.
pub fn read_zbc_namespace(data: &[u8]) -> Result<String> {
    use anyhow::bail;
    if data.len() < 16 { bail!("zbc buffer too short ({} bytes)", data.len()) }
    if &data[0..4] != ZBC_MAGIC { bail!("not a zbc file (bad magic)") }

    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = super::zbc_reader::read_directory_pub(data, sec_count)?;

    match dir.get(b"NSPC") {
        None => Ok(String::new()),
        Some(&(off, size)) => {
            if off + size > data.len() { bail!("NSPC section out of bounds") }
            let sec = &data[off..off + size];
            if sec.len() < 2 { return Ok(String::new()); }
            let len = u16::from_le_bytes([sec[0], sec[1]]) as usize;
            if len == 0 || sec.len() < 2 + len { return Ok(String::new()); }
            Ok(std::str::from_utf8(&sec[2..2 + len])?.to_owned())
        }
    }
}

#[cfg(test)]
#[path = "namespace_index_tests.rs"]
mod tests;
