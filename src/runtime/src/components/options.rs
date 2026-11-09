//! Component selection and automatic attachment of same-build runtime components.
use super::catalog::{Catalog, Registration};
use super::VmComponents;
use crate::context::VmContext;
use anyhow::{bail, Result};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use crate::components::api::*;

/// Immutable component selection. Clones share the factory load result; each VM
/// receives new instances. Rebuild these options to retry a failed library load.
#[derive(Clone, Default)]
pub struct ComponentOptions {
    registrations: Vec<Registration>,
    catalog: Option<Arc<OnceLock<Result<Arc<Catalog>, String>>>>,
}
impl ComponentOptions {
    /// Select a statically linked factory. Its direct function reference keeps it
    /// reachable to the linker; loading a program automatically initializes it.
    ///
    /// # Safety
    /// init and every returned descriptor span/table/callback must satisfy the
    /// component ABI, concurrency and lifetime contracts. No unwinding is allowed.
    pub unsafe fn with_static(mut self, id: ComponentId, init: ComponentInit) -> Self {
        self.registrations.push(Registration::Static { id, init, cache: Default::default() });
        self.catalog = Some(Arc::new(OnceLock::new()));
        self
    }
    /// Select a trusted native library and its C descriptor entrypoint.
    ///
    /// # Safety
    /// The library and symbol must implement the component ABI. Loading native code
    /// executes its initializers; its descriptors/callbacks obey with_static's contract.
    pub unsafe fn with_dynamic(
        mut self,
        id: ComponentId,
        path: impl Into<PathBuf>,
        symbol: impl Into<String>,
    ) -> Self {
        self.registrations.push(Registration::Dynamic {
            id,
            path: path.into(),
            symbol: symbol.into(),
            cache: Default::default(),
        });
        self.catalog = Some(Arc::new(OnceLock::new()));
        self
    }
    pub(crate) fn merged(&self, extra: &Self) -> Self {
        let mut registrations = self.registrations.clone();
        registrations.extend(extra.registrations.iter().cloned());
        Self { registrations, catalog: Some(Arc::new(OnceLock::new())) }
    }
    pub(crate) fn install(&self, ctx: &VmContext) -> Result<()> {
        if ctx.core().components().get().is_some() {
            bail!("VM component registrations are already frozen");
        }
        let components = match &self.catalog {
            None => VmComponents::create(&[], ctx.heap().heap_epoch())?,
            Some(cell) => {
                let catalog = cell.get_or_init(|| {
                    Catalog::load(&self.registrations)
                        .map(Arc::new)
                        .map_err(|e| format!("{e:#}"))
                });
                let catalog = catalog.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
                VmComponents::create(&catalog.factories, ctx.heap().heap_epoch())?
            }
        };
        ctx.core()
            .components()
            .set(components)
            .map_err(|_| anyhow::anyhow!("VM component registrations are already frozen"))
    }
    #[cfg(test)]
    pub(super) fn catalog_for_test(&self) -> Arc<Catalog> {
        self.catalog
            .as_ref()
            .unwrap()
            .get_or_init(|| Catalog::load(&self.registrations).map(Arc::new).map_err(|e| format!("{e:#}")))
            .as_ref()
            .unwrap()
            .clone()
    }
}

/// Query a registered component without transferring its state or library ownership.
/// The returned lease keeps the VM alive until all calls through the table finish.
pub fn component_interface<T: ComponentInterface>(
    ctx: &VmContext,
    id: ComponentId,
) -> Result<Option<super::ComponentLease<T>>> {
    match ctx.core().components().get() {
        Some(components) => components.lease(id, ctx.core_arc()),
        None => Ok(None),
    }
}
/// Registered components in deterministic dependency order. Disabled components are absent.
pub fn component_ids(ctx: &VmContext) -> Vec<ComponentId> {
    ctx.core()
        .components()
        .get()
        .map_or_else(Vec::new, VmComponents::ids)
}
