//! Platform-isolated filesystem backend (add-wasm-vfs-backend).
//!
//! `fs.rs`'s path-based builtins are **platform-agnostic** — they call
//! `fs_backend::active().read(path)` etc., never `std::fs` directly. Each
//! platform's implementation lives isolated in its own module:
//!   - [`native`] — `std::fs` (default on non-wasm; byte-identical to before)
//!   - [`memory`] — `path → bytes` VFS (default on wasm; host mounts zpkgs)
//!
//! Selection: `cfg(wasm32)` picks the default (Memory / Native); `set_backend`
//! overrides at runtime — so the VFS↔disk consistency test runs on native, and
//! wasm can later swap Memory ↔ a JS-callback backend. fs is I/O-bound and
//! low-frequency, so the one `match` per call is free.
use anyhow::Result;
use std::sync::atomic::{AtomicU8, Ordering};

pub mod memory;
pub mod native;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FsBackend {
    Native,
    Memory,
}

const NATIVE: u8 = 0;
const MEMORY: u8 = 1;

// Default is compile-time (wasm → Memory, else Native); `set_backend` overrides.
static ACTIVE: AtomicU8 = AtomicU8::new(default_code());

const fn default_code() -> u8 {
    #[cfg(target_arch = "wasm32")]
    {
        MEMORY
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        NATIVE
    }
}

pub fn set_backend(b: FsBackend) {
    let code = match b {
        FsBackend::Native => NATIVE,
        FsBackend::Memory => MEMORY,
    };
    ACTIVE.store(code, Ordering::Relaxed);
}

/// The active backend (dispatch surface). Copy — cheap to fetch per call.
pub fn active() -> FsBackend {
    match ACTIVE.load(Ordering::Relaxed) {
        MEMORY => FsBackend::Memory,
        _ => FsBackend::Native,
    }
}

/// Dispatch each op to the isolated per-platform impl. Adding a WASI/mobile
/// backend later = one more module + arms here; the builtins never change.
impl FsBackend {
    pub fn read_to_string(self, path: &str) -> Result<String> {
        match self { FsBackend::Native => native::read_to_string(path), FsBackend::Memory => memory::read_to_string(path) }
    }
    pub fn read(self, path: &str) -> Result<Vec<u8>> {
        match self { FsBackend::Native => native::read(path), FsBackend::Memory => memory::read(path) }
    }
    pub fn write(self, path: &str, bytes: &[u8]) -> Result<()> {
        match self { FsBackend::Native => native::write(path, bytes), FsBackend::Memory => memory::write(path, bytes) }
    }
    pub fn append(self, path: &str, bytes: &[u8]) -> Result<()> {
        match self { FsBackend::Native => native::append(path, bytes), FsBackend::Memory => memory::append(path, bytes) }
    }
    pub fn exists(self, path: &str) -> bool {
        match self { FsBackend::Native => native::exists(path), FsBackend::Memory => memory::exists(path) }
    }
    pub fn is_dir(self, path: &str) -> bool {
        match self { FsBackend::Native => native::is_dir(path), FsBackend::Memory => memory::is_dir(path) }
    }
    pub fn remove_file(self, path: &str) -> Result<()> {
        match self { FsBackend::Native => native::remove_file(path), FsBackend::Memory => memory::remove_file(path) }
    }
    pub fn remove_dir(self, path: &str, recursive: bool) -> Result<()> {
        match self { FsBackend::Native => native::remove_dir(path, recursive), FsBackend::Memory => memory::remove_dir(path, recursive) }
    }
    pub fn copy(self, src: &str, dst: &str) -> Result<()> {
        match self { FsBackend::Native => native::copy(src, dst), FsBackend::Memory => memory::copy(src, dst) }
    }
    pub fn rename(self, src: &str, dst: &str) -> Result<()> {
        match self { FsBackend::Native => native::rename(src, dst), FsBackend::Memory => memory::rename(src, dst) }
    }
    pub fn create_dir_all(self, path: &str) -> Result<()> {
        match self { FsBackend::Native => native::create_dir_all(path), FsBackend::Memory => memory::create_dir_all(path) }
    }
    pub fn modified_ms(self, path: &str) -> Result<i64> {
        match self { FsBackend::Native => native::modified_ms(path), FsBackend::Memory => memory::modified_ms(path) }
    }
    pub fn file_len(self, path: &str) -> Result<u64> {
        match self { FsBackend::Native => native::file_len(path), FsBackend::Memory => memory::file_len(path) }
    }
    pub fn read_dir(self, path: &str) -> Result<Vec<String>> {
        match self { FsBackend::Native => native::read_dir(path), FsBackend::Memory => memory::read_dir(path) }
    }
    pub fn glob(self, dir: &str, pattern: &str) -> Result<Vec<String>> {
        match self { FsBackend::Native => native::glob(dir, pattern), FsBackend::Memory => memory::glob(dir, pattern) }
    }
    pub fn write_atomic(self, path: &str, bytes: &[u8]) -> Result<()> {
        match self { FsBackend::Native => native::write_atomic(path, bytes), FsBackend::Memory => memory::write_atomic(path, bytes) }
    }
}
