//! Reflection context operations keep registry guards inside Loader.
use super::RuntimeLoader;
use crate::loader::contexts::{AssemblyId, ContextId, ContextLoadError, ContextState, UnloadOutcome};
use crate::object::TypeDesc;
use std::sync::Arc;

impl RuntimeLoader<'_> {
    pub(crate) fn create_collectible_context(&self, name: &str) -> ContextId {
        self.state.context_registry.lock().create_collectible(name)
    }

    /// Load into the context's private arena for reflection.
    pub(crate) fn load_context_artifact(
        &self,
        context: ContextId,
        path: &str,
    ) -> Result<AssemblyId, ContextLoadError> {
        if self.state.context_registry.lock().context_state(context) != Some(ContextState::Active) {
            return Err(ContextLoadError::Inactive);
        }
        let artifact = crate::loader::package::load_artifact_with_fusion(
            self.state.vfs.as_ref(),
            path,
            self.state.fusion_inputs,
        )
        .map_err(ContextLoadError::Artifact)?;
        let name = std::path::Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        Ok(self.state.context_registry.lock().load_into(context, name, artifact.module))
    }

    pub(crate) fn unload_context(&self, context: ContextId) -> UnloadOutcome {
        self.state.context_registry.lock().unload(context)
    }

    pub(crate) fn context_name(&self, context: ContextId) -> Option<String> {
        self.state.context_registry.lock().context_name(context)
    }

    pub(crate) fn context_is_collectible(&self, context: ContextId) -> Option<bool> {
        self.state.context_registry.lock().context_is_collectible(context)
    }

    pub(crate) fn context_assemblies(&self, context: ContextId) -> Vec<AssemblyId> {
        self.state.context_registry.lock().context_assemblies(context)
    }

    pub(crate) fn assembly_name(&self, assembly: AssemblyId) -> Option<String> {
        self.state.context_registry.lock().assembly_name(assembly)
    }

    pub(crate) fn assembly_is_collectible(&self, assembly: AssemblyId) -> bool {
        self.state.context_registry.lock().assembly_is_collectible(assembly)
    }

    pub(crate) fn assembly_context(&self, assembly: AssemblyId) -> Option<ContextId> {
        self.state.context_registry.lock().assembly_context(assembly)
    }

    pub(crate) fn assembly_types(&self, assembly: AssemblyId) -> Vec<Arc<TypeDesc>> {
        self.state.context_registry.lock().assembly_types(assembly)
    }

    pub(crate) fn assembly_function_names(&self, assembly: AssemblyId) -> Vec<String> {
        self.state.context_registry.lock().assembly_function_names(assembly)
    }
}
