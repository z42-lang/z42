//! Per-VM module linkage and link-time copy-on-write views of package data.
use super::Function;
use crate::metadata::image::{ModuleData, PackageImage};
use crate::{metadata::ir, object::TypeDesc};
use rustc_hash::FxHashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
type TypeRegistry = FxHashMap<String, Arc<TypeDesc>>;
#[derive(Debug)]
pub struct Module {
    data: Arc<ModuleData>,
    pub functions: Vec<Function>,
    pub type_registry: TypeRegistry,
}
impl Module {
    pub fn new(data: ModuleData, functions: Vec<Function>, type_registry: TypeRegistry) -> Self {
        Self {
            data: Arc::new(data),
            functions,
            type_registry,
        }
    }
    pub fn from_ir(module: ir::Module) -> Self {
        Self::new(
            ModuleData {
                name: module.name,
                string_pool: module.string_pool,
                classes: module.classes,
                func_index: module.func_index,
            },
            module.functions.into_iter().map(Function::new).collect(),
            Default::default(),
        )
    }
    pub fn from_image(image: Arc<PackageImage>) -> Self {
        Self {
            data: image.data(),
            functions: image
                .definitions()
                .iter()
                .cloned()
                .map(Function::from_definition)
                .collect(),
            type_registry: Default::default(),
        }
    }
    /// Capture definitions only. The image never retains this module's type registry or caches.
    pub fn image(&self) -> Arc<PackageImage> {
        PackageImage::snapshot(
            self.data.clone(),
            self.functions
                .iter()
                .map(Function::shared_definition)
                .collect(),
        )
    }
    pub fn into_image(self) -> Arc<PackageImage> {
        self.image()
    }
    pub(crate) fn into_parts(self) -> (ModuleData, Vec<Function>, TypeRegistry) {
        let Self {
            data,
            functions,
            type_registry,
        } = self;
        (Arc::unwrap_or_clone(data), functions, type_registry)
    }
    pub(crate) fn clear_link_state(&mut self) {
        for function in &mut self.functions {
            function.clear_link_state();
        }
    }
}
impl Deref for Module {
    type Target = ModuleData;
    fn deref(&self) -> &ModuleData {
        &self.data
    }
}
impl DerefMut for Module {
    fn deref_mut(&mut self) -> &mut ModuleData {
        Arc::make_mut(&mut self.data)
    }
}
