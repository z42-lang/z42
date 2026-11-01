//! Immutable package definitions, shared without VM identities or caches.
//!
//! ```compile_fail
//! fn mutate(image: &mut z42::metadata::image::PackageImage) {
//!     image.name.clear();
//! }
//! ```
use super::ir::{ClassDesc, Function, Module};
use rustc_hash::FxHashMap;
use std::ops::Deref;
use std::sync::Arc;

/// Static module-level data. PackageImage exposes a shared view, never a mutable one.
#[derive(Debug, Clone)]
pub struct ModuleData {
    pub name: String,
    pub string_pool: Vec<String>,
    pub classes: Vec<ClassDesc>,
    pub func_index: FxHashMap<String, usize>,
}

/// A frozen package. Mutable linkage belongs to the VM that instantiates it.
#[derive(Debug)]
pub struct PackageImage {
    data: Arc<ModuleData>,
    functions: Vec<Arc<Function>>,
}
impl PackageImage {
    /// Freeze decoded definitions. This performs no I/O or VM registration.
    pub fn new(mut module: Module) -> Arc<Self> {
        for f in &mut module.functions {
            f.prepare();
        }
        module.func_index = module
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        Arc::new(Self {
            data: Arc::new(ModuleData {
                name: module.name,
                string_pool: module.string_pool,
                classes: module.classes,
                func_index: module.func_index,
            }),
            functions: module.functions.into_iter().map(Arc::new).collect(),
        })
    }
    pub fn functions(&self) -> impl ExactSizeIterator<Item = &Function> {
        self.functions.iter().map(AsRef::as_ref)
    }
    pub fn function(&self, index: usize) -> Option<&Function> {
        self.functions.get(index).map(AsRef::as_ref)
    }
    pub(crate) fn data(&self) -> Arc<ModuleData> {
        self.data.clone()
    }
    pub(crate) fn definitions(&self) -> &[Arc<Function>] {
        &self.functions
    }
    pub(crate) fn snapshot(data: Arc<ModuleData>, functions: Vec<Arc<Function>>) -> Arc<Self> {
        let mut data = data;
        if data.func_index.len() != functions.len()
            || functions
                .iter()
                .enumerate()
                .any(|(i, f)| data.func_index.get(&f.name) != Some(&i))
        {
            Arc::make_mut(&mut data).func_index = functions
                .iter()
                .enumerate()
                .map(|(i, f)| (f.name.clone(), i))
                .collect();
        }
        Arc::new(Self { data, functions })
    }
}
impl Deref for PackageImage {
    type Target = ModuleData;
    fn deref(&self) -> &ModuleData {
        &self.data
    }
}
