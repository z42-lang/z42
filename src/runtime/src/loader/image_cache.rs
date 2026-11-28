//! Session-scoped zpkg decode cache: one decoded decoded per distinct
//! content, shared by every program loaded through the same session (the C#
//! ALC "already loaded ⇒ shared" rule). Function IR and the merged definition
//! data are `Arc`-shared across programs; per-VM type tables, statics and
//! dispatch caches remain per VM — those are instantiation state, not
//! definitions, exactly as distinct ALCs each get their own RuntimeTypes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rustc_hash::FxHashMap;

use crate::metadata::Module;
use crate::zpkg::image::PackageImage;

/// Immutable metadata a loader needs beyond the definitions themselves.
/// Small, cloned on every cache hit; the heavy parts stay inside the image.
#[derive(Clone)]
pub(crate) struct ProgramMetadata {
    pub entry_hint: Option<String>,
    pub dependencies: Vec<crate::zpkg::format::ZpkgDep>,
    pub import_namespaces: Vec<String>,
    pub impl_pairs: Vec<(String, String)>,
    pub package_name: Option<String>,
    pub content_id: Option<String>,
    /// The decoded module's type registry. `Module::from_image` rebuilds a
    /// module with an EMPTY registry; per-package registration (M1+) consumes
    /// the decoded program's registry directly, so the cache carries it alongside
    /// the image and restores it on every handout.
    pub type_registry: FxHashMap<String, std::sync::Arc<crate::object::TypeDesc>>,
}

/// Content-addressed decoded cache: key = hash of the raw program bytes.
/// Content addressing makes the cache safe against a same-path-different-
/// content reload and makes repeat loads of equal bytes (resolver or file)
/// share unconditionally.
#[derive(Default)]
pub struct ImageCache {
    programs: Mutex<HashMap<String, (Arc<PackageImage>, ProgramMetadata)>>,
}

impl ImageCache {
    /// Decode-or-share: on a hit the decoded program carries a shallow
    /// `from_image` module (definition `Arc`s shared with every earlier
    /// program of this session); on a miss the freshly decoded module is
    /// frozen into the cache and handed back the same way. Either way the
    /// caller owns only per-VM linkage state.
    pub fn program_for(
        &self,
        content_id: &str,
        decode: impl FnOnce() -> Result<crate::loader::DecodedProgram>,
    ) -> Result<crate::loader::DecodedProgram> {
        let cached = self.programs.lock().unwrap().get(content_id).cloned();
        match cached {
            // Hit: shallow view over the shared image with the decoded type
            // registry restored (`from_image` alone yields an empty one).
            Some((image, meta)) => {
                let mut module = Module::from_image(image);
                module.type_registry = meta.type_registry;
                Ok(crate::loader::DecodedProgram {
                    module,
                    entry_hint: meta.entry_hint,
                    dependencies: meta.dependencies,
                    import_namespaces: meta.import_namespaces,
                    impl_pairs: meta.impl_pairs,
                    package_name: meta.package_name,
                    content_id: meta.content_id,
                })
            }
            // Miss: decode, freeze the image + the decoded type registry
            // into the cache, and hand back the SAME shallow shape a hit
            // produces (every handout goes through `from_image`, so the
            // definition `Arc`s are shared across the whole session; the
            // registry is restored because per-package registration consumes
            // the decoded program's registry directly).
            None => {
                let decoded = decode()?;
                let image = decoded.module.image();
                let meta = ProgramMetadata {
                    entry_hint: decoded.entry_hint.clone(),
                    dependencies: decoded.dependencies.clone(),
                    import_namespaces: decoded.import_namespaces.clone(),
                    impl_pairs: decoded.impl_pairs.clone(),
                    package_name: decoded.package_name.clone(),
                    content_id: decoded.content_id.clone(),
                    type_registry: decoded.module.type_registry.clone(),
                };
                self.programs
                    .lock()
                    .unwrap()
                    .entry(content_id.to_owned())
                    .or_insert_with(|| (image.clone(), meta.clone()));
                let mut module = Module::from_image(image);
                module.type_registry = meta.type_registry;
                Ok(crate::loader::DecodedProgram {
                    module,
                    entry_hint: meta.entry_hint,
                    dependencies: meta.dependencies,
                    import_namespaces: meta.import_namespaces,
                    impl_pairs: meta.impl_pairs,
                    package_name: meta.package_name,
                    content_id: meta.content_id,
                })
            }
        }
    }

    /// Number of distinct programs held — test/diagnostics surface.
    pub fn len(&self) -> usize {
        self.programs.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.programs.lock().unwrap().is_empty()
    }
}

/// Content identity of raw decoded bytes. Equal content ⇒ equal key ⇒ one
/// shared image. (`blake3[..16]` hex, matching loader/package/decode.rs.)
pub fn content_id(bytes: &[u8]) -> String {
    let digest = blake3::hash(bytes);
    let mut hex = String::with_capacity(32);
    for byte in &digest.as_bytes()[..16] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}
