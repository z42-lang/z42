//! In-memory VFS (wasm-vfs-spike) — route fs builtins to a `path → bytes` map so
//! `DepScan` / z42c / scripting can compile against in-memory zpkgs **without a real
//! filesystem** (browser/wasm). The whole point: `fs.rs` is the single fs chokepoint,
//! so routing its read/exists/glob to this VFS lets DepScan run **unchanged** on wasm.
//!
//! Spike form: a process-global map + an enable flag. Native default = off (std::fs);
//! the driver mounts zpkgs then `__vfs_enable()`. In real wasm this becomes the default
//! backend and JS mounts the stdlib + z42c zpkgs (wasm-bindgen). A production version
//! would be an `FsBackend` trait (NativeFs / MemoryVfs / JsCallbackFs), see proposal.
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static VFS_ENABLED: AtomicBool = AtomicBool::new(false);

fn store() -> &'static RwLock<HashMap<String, Vec<u8>>> {
    static VFS: OnceLock<RwLock<HashMap<String, Vec<u8>>>> = OnceLock::new();
    VFS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Are fs builtins routed to the in-memory VFS? (false = std::fs)
pub fn enabled() -> bool {
    VFS_ENABLED.load(Ordering::Relaxed)
}

/// Read a mounted file's bytes (None if not mounted).
pub fn read(path: &str) -> Option<Vec<u8>> {
    store().read().get(path).cloned()
}

/// True if any mounted file lives directly-or-deeper under `dir`.
pub fn dir_exists(dir: &str) -> bool {
    let d = dir.trim_end_matches('/');
    store().read().keys().any(|k| match k.strip_prefix(d) {
        Some(rest) => rest.starts_with('/'),
        None => false,
    })
}

/// Direct children of `dir` whose basename matches `pattern` (full paths, sorted).
pub fn glob(dir: &str, pattern: &str) -> Vec<String> {
    let d = dir.trim_end_matches('/');
    let mut hits: Vec<String> = Vec::new();
    for k in store().read().keys() {
        if let Some(rest) = k.strip_prefix(d) {
            let name = rest.strip_prefix('/').unwrap_or(rest);
            if !name.is_empty() && !name.contains('/') && super::fs::glob_match(pattern, name) {
                hits.push(k.clone());
            }
        }
    }
    hits.sort();
    hits
}

/// `__vfs_mount(path: string, bytes: byte[])` — add a virtual file.
pub fn builtin_vfs_mount(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__vfs_mount: arg 0 must be a path string"),
    };
    let bytes: Vec<u8> = match args.get(1) {
        Some(Value::Array(a)) => a
            .borrow()
            .iter()
            .map(|v| match v {
                Value::I64(n) => (*n & 0xff) as u8,
                _ => 0,
            })
            .collect(),
        _ => bail!("__vfs_mount: arg 1 must be byte[]"),
    };
    store().write().insert(path, bytes);
    Ok(Value::Null)
}

/// `__vfs_enable()` — route fs builtins to the VFS from now on.
pub fn builtin_vfs_enable(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    VFS_ENABLED.store(true, Ordering::Relaxed);
    Ok(Value::Null)
}
