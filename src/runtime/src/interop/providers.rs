//! Native providers publish immutable descriptors; lookup never loads or executes code.
use crate::exec::call::builtin_abi::BuiltinDesc;
use crate::metadata::{
    builtins::{BuiltinCatalog, BuiltinInfo},
    ids::tokens::BuiltinId,
};
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
pub struct NativeProvider {
    pub name: &'static str,
    pub functions: &'static [BuiltinDesc],
}

#[derive(Default)]
pub struct RegistryBuilder {
    providers: Vec<NativeProvider>,
    modules: Vec<std::sync::Arc<super::module::NativeModule>>,
}

impl RegistryBuilder {
    /// Validate the complete provider before publishing any of its names.
    pub fn register(&mut self, provider: NativeProvider) -> Result<()> {
        if provider.name.is_empty() {
            bail!("native provider name must not be empty");
        }
        if self.providers.iter().any(|p| p.name == provider.name)
            || self
                .modules
                .iter()
                .any(|module| module.name == provider.name)
        {
            bail!("duplicate native provider `{}`", provider.name);
        }
        let mut names = HashSet::new();
        for function in provider.functions {
            if function.name.is_empty() {
                bail!("empty function name in native provider `{}`", provider.name);
            }
            if !names.insert(function.name) {
                bail!(
                    "duplicate native function `{}` in provider `{}`",
                    function.name,
                    provider.name
                );
            }
        }
        self.providers.push(provider);
        Ok(())
    }

    /// Register the same descriptor supplied by a static or dynamic native module entry.
    ///
    /// # Safety
    /// See NativeModule::parse: descriptor/code lifetime and callback contracts apply.
    pub unsafe fn register_module(
        &mut self,
        descriptor: *const z42_abi::native::Module,
    ) -> Result<()> {
        let module = unsafe { super::module::NativeModule::parse(descriptor)? };
        if self
            .providers
            .iter()
            .any(|provider| provider.name == module.name)
            || self
                .modules
                .iter()
                .any(|registered| registered.name == module.name)
        {
            bail!("duplicate native provider `{}`", module.name);
        }
        self.modules.push(std::sync::Arc::new(module));
        Ok(())
    }

    pub fn freeze(self) -> NativeRegistry {
        let mut functions = Vec::new();
        let mut providers = HashMap::new();
        for provider in self.providers {
            let start = functions.len();
            functions.extend_from_slice(provider.functions);
            providers.insert(provider.name, start..functions.len());
        }
        let catalog = BuiltinCatalog::from_records(
            functions
                .iter()
                .map(|function| BuiltinInfo {
                    name: function.name,
                    frameless: !function.flags.reenters,
                })
                .collect(),
        )
        .with_reserved_names(
            self.modules
                .iter()
                .flat_map(|module| module.functions.keys().copied())
                .collect(),
        );
        NativeRegistry {
            functions: functions.into_boxed_slice(),
            catalog,
            providers,
            modules: self
                .modules
                .into_iter()
                .map(|module| (module.name, module))
                .collect(),
        }
    }
}

/// Immutable registration result. Catalog IDs and execution entries share one ordering.
pub struct NativeRegistry {
    functions: Box<[BuiltinDesc]>,
    catalog: BuiltinCatalog,
    providers: HashMap<&'static str, std::ops::Range<usize>>,
    modules: HashMap<&'static str, std::sync::Arc<super::module::NativeModule>>,
}

impl NativeRegistry {
    pub fn functions(&self) -> &[BuiltinDesc] {
        &self.functions
    }
    pub fn catalog(&self) -> &BuiltinCatalog {
        &self.catalog
    }

    pub fn module(&self, name: &str) -> Option<std::sync::Arc<super::module::NativeModule>> {
        self.modules.get(name).cloned()
    }
    pub fn modules(&self) -> impl Iterator<Item = &std::sync::Arc<super::module::NativeModule>> {
        self.modules.values()
    }

    pub fn provider_functions(&self, provider: &str) -> Option<&[BuiltinDesc]> {
        self.providers
            .get(provider)
            .map(|range| &self.functions[range.clone()])
    }

    pub fn resolve(&self, provider: &str, name: &str) -> Option<BuiltinId> {
        let range = self.providers.get(provider)?;
        let offset = self.functions[range.clone()]
            .iter()
            .position(|function| function.name == name)?;
        Some(BuiltinId((range.start + offset) as u32))
    }
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
