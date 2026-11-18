//! Session-scoped zpkg decode cache: one decoded artifact per distinct
//! content, shared by every program loaded through the same session (the C#
//! ALC "already loaded ⇒ shared" rule). Function IR and the merged definition
//! data are `Arc`-shared across programs; per-VM type tables, statics and
//! dispatch caches remain per VM — those are instantiation state, not
//! definitions, exactly as distinct ALCs each get their own RuntimeTypes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::metadata::Module;
use crate::zpkg::image::PackageImage;

/// Immutable metadata a loader needs beyond the definitions themselves.
/// Small, cloned on every cache hit; the heavy parts stay inside the image.
#[derive(Clone)]
pub(crate) struct ArtifactMeta {
    pub entry_hint: Option<String>,
    pub dependencies: Vec<crate::zpkg::format::ZpkgDep>,
    pub import_namespaces: Vec<String>,
    pub impl_pairs: Vec<(String, String)>,
    pub package_name: Option<String>,
}

/// Content-addressed artifact cache: key = hash of the raw artifact bytes.
/// Content addressing makes the cache safe against a same-path-different-
/// content reload and makes repeat loads of equal bytes (resolver or file)
/// share unconditionally.
#[derive(Default)]
pub struct ImageCache {
    artifacts: Mutex<HashMap<String, (Arc<PackageImage>, ArtifactMeta)>>,
}

impl ImageCache {
    /// Decode-or-share: on a hit the returned artifact carries a shallow
    /// `from_image` module (definition `Arc`s shared with every earlier
    /// program of this session); on a miss the freshly decoded module is
    /// frozen into the cache and handed back the same way. Either way the
    /// caller owns only per-VM linkage state.
    pub fn artifact_for(
        &self,
        content_id: &str,
        decode: impl FnOnce() -> Result<crate::loader::LoadedArtifact>,
    ) -> Result<crate::loader::LoadedArtifact> {
        let cached = self.artifacts.lock().unwrap().get(content_id).cloned();
        let (image, meta) = match cached {
            Some(pair) => pair,
            None => {
                let artifact = decode()?;
                let meta = ArtifactMeta {
                    entry_hint: artifact.entry_hint.clone(),
                    dependencies: artifact.dependencies.clone(),
                    import_namespaces: artifact.import_namespaces.clone(),
                    impl_pairs: artifact.impl_pairs.clone(),
                    package_name: artifact.package_name.clone(),
                };
                let image = artifact.module.into_image();
                let entry = self
                    .artifacts
                    .lock()
                    .unwrap()
                    .entry(content_id.to_owned())
                    .or_insert_with(|| (image.clone(), meta.clone()))
                    .clone();
                entry
            }
        };
        Ok(crate::loader::LoadedArtifact {
            module: Module::from_image(image),
            entry_hint: meta.entry_hint,
            dependencies: meta.dependencies,
            import_namespaces: meta.import_namespaces,
            impl_pairs: meta.impl_pairs,
            package_name: meta.package_name,
        })
    }

    /// Number of distinct artifacts held — test/diagnostics surface.
    pub fn len(&self) -> usize {
        self.artifacts.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.artifacts.lock().unwrap().is_empty()
    }
}

/// Content identity of raw artifact bytes. Equal content ⇒ equal key ⇒ one
/// shared image. (`blake3[..16]` hex, matching loader/package/artifact.rs.)
pub fn content_id(bytes: &[u8]) -> String {
    let digest = blake3::hash(bytes);
    let mut hex = String::with_capacity(32);
    for byte in &digest.as_bytes()[..16] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}
