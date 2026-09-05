//! 内置 [`ZpkgResolver`] 实现。
//!
//! 两个自包含的类型，与 `lib.rs` 的 Host 生命周期管理无关，变更频率也不同。
//! 拆出来是行数硬限所迫（`lib.rs` 长期在 line-limit 棘轮基线上，越界文件不得增长），
//! 顺便让 `lib.rs` 回到 500 行以下、可以从基线剔除。
//! app-config-follows-the-app（2026-09-05）的顺带整理。

use std::path::PathBuf;
use std::sync::Arc;

use crate::ZpkgResolver;

// ── Built-in resolvers ──────────────────────────────────────────────────

/// `HashMap`-backed eager resolver. The host pre-populates all known
/// zpkgs at startup; `resolve` is a trivial map lookup. Ideal for
/// mobile / WASM where stdlib bundles are loaded once and then served
/// from memory.
///
/// ```no_run
/// # use z42_host::{MapResolver};
/// let mut r = MapResolver::new();
/// r.insert("z42.core", std::fs::read("z42.core.zpkg").unwrap());
/// ```
pub struct MapResolver {
    map: std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>,
}

impl MapResolver {
    pub fn new() -> Self {
        Self {
            map: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn insert(&self, namespace: &str, bytes: Vec<u8>) {
        if let Ok(mut g) = self.map.write() {
            g.insert(namespace.to_string(), bytes);
        }
    }

    pub fn with(namespace: &str, bytes: Vec<u8>) -> Arc<Self> {
        let r = Self::new();
        r.insert(namespace, bytes);
        Arc::new(r)
    }
}

impl Default for MapResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ZpkgResolver for MapResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        self.map.read().ok()?.get(namespace).cloned()
    }
}

/// Filesystem-based resolver. Scans the configured directories for
/// `.zpkg` files declaring the requested namespace (mirrors the legacy
/// `search_paths` behaviour). Useful for desktop apps that ship the
/// stdlib alongside the binary and want explicit resolver chaining.
pub struct SearchPathsResolver {
    paths: Vec<PathBuf>,
}

impl SearchPathsResolver {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }
}

impl ZpkgResolver for SearchPathsResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        let zpkgs = z42::metadata::resolve_namespace(namespace, &[], &self.paths).ok()?;
        for zpkg_path in zpkgs {
            if let Ok(bytes) = std::fs::read(&zpkg_path) {
                return Some(bytes);
            }
        }
        None
    }
}
