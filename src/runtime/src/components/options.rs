//! Component selection and automatic attachment of same-build runtime components.
use super::catalog::{Catalog, Registration};
use super::instance::VmComponents;
#[cfg(test)]
use super::instance::ComponentLease;
use crate::vm::VmContext;
use anyhow::{bail, Result};
use std::{
    path::PathBuf,
};
use crate::components::abi::*;
use super::execution_roles::ExecutionBindings;
#[cfg(test)]
use std::sync::Arc;

/// Immutable component selection. Clones share the factory load result; each VM
/// receives new instances. Rebuild these options to retry a failed library load.
#[derive(Clone, Default)]
pub struct ComponentOptions {
    registrations: Vec<Registration>,
    roots: Vec<ComponentId>,
}
impl ComponentOptions {
    /// Select a statically linked factory. Its direct function reference keeps it
    /// reachable to the linker; loading a program automatically initializes it.
    ///
    /// # Safety
    /// init and every returned descriptor span/table/callback must satisfy the
    /// component ABI, concurrency and lifetime contracts. No unwinding is allowed.
    #[cfg(test)]
    pub unsafe fn with_static(mut self, id: ComponentId, init: ComponentInit) -> Self {
        self = unsafe { self.with_static_provider(id, init) };
        self.roots.push(id);
        self
    }
    pub(super) unsafe fn with_linked(mut self, id: ComponentId, init: ComponentInit, origin: crate::loader::NativeOrigin) -> Self {
        self.registrations.push(Registration::Linked { id, init, origin, cache: Default::default() });
        self.roots.push(id);
        self
    }
    /// Make a static factory available to declared dependencies without enabling it.
    /// Unused providers are never initialized and do not create VM instances.
    ///
    /// # Safety
    /// The factory obeys the same ABI and lifetime contract as [`Self::with_static`].
    #[cfg(test)]
    pub unsafe fn with_static_provider(mut self, id: ComponentId, init: ComponentInit) -> Self {
        self.registrations.push(Registration::Linked { id, init, origin: crate::loader::NativeOrigin::StaticLink, cache: Default::default() });
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
        self = unsafe { self.with_dynamic_provider(id, path, symbol) };
        self.roots.push(id);
        self
    }
    /// Make a dynamic factory available to declared dependencies without opening
    /// its library until an enabled component requires it.
    ///
    /// # Safety
    /// The library and symbol obey [`Self::with_dynamic`]'s native-code contract.
    pub unsafe fn with_dynamic_provider(
        mut self,
        id: ComponentId,
        path: impl Into<PathBuf>,
        symbol: impl Into<String>,
    ) -> Self {
        self.registrations.push(Registration::Runtime {
            id,
            path: path.into(),
            symbol: symbol.into(),
            cache: Default::default(),
        });
        self
    }
    pub(crate) fn merged(&self, extra: &Self) -> Self {
        let mut registrations = self.registrations.clone();
        registrations.extend(extra.registrations.iter().cloned());
        let mut roots = self.roots.clone();
        roots.extend(extra.roots.iter().copied());
        Self { registrations, roots }
    }
    #[cfg(test)]
    pub(super) fn install(&self, ctx: &VmContext) -> Result<()> {
        self.install_bound(ctx, ExecutionBindings::default())
    }
    pub(super) fn install_bound(&self, ctx: &VmContext, execution: ExecutionBindings) -> Result<()> {
        if ctx.core().components().get().is_some() {
            bail!("VM component registrations are already frozen");
        }
        let components = self.create_bound(ctx.heap().heap_epoch(), execution, ctx.loader().native_loading())?;
        ctx.core()
            .components()
            .set(components)
            .map_err(|_| anyhow::anyhow!("VM component registrations are already frozen"))
    }
    pub(super) fn create(&self, vm_id: u64, policy: &crate::loader::NativeLoading) -> Result<VmComponents> {
        self.create_bound(vm_id, ExecutionBindings::default(), policy)
    }
    fn create_bound(&self, vm_id: u64, execution: ExecutionBindings, policy: &crate::loader::NativeLoading) -> Result<VmComponents> {
        // Recheck admission on every VM, even when factory/code caches are warm.
        // Policy rejection must not populate a shared initialization failure cache.
        let catalog = Catalog::load(&self.registrations, &self.roots, policy)?;
        VmComponents::create_bound(&catalog.factories, vm_id, execution)
    }
    #[cfg(test)]
    pub(super) fn catalog_for_test(&self) -> Arc<Catalog> {
        Arc::new(Catalog::load(&self.registrations, &self.roots, &Default::default()).unwrap())
    }

}

/// Query a registered component without transferring its state or library ownership.
/// The returned lease keeps the VM alive until all calls through the table finish.
#[cfg(test)]
pub fn component_interface<T: ComponentInterface>(
    ctx: &VmContext,
    id: ComponentId,
) -> Result<Option<ComponentLease<T>>> {
    if let Some(components) = ctx.core().components().get() {
        let interface = components.lease(id, ctx.core_arc())?;
        if interface.is_some() { return Ok(interface); }
    }
    match ctx.core().profiling().components() {
        Some(components) => components.lease(id, ctx.core_arc()),
        None => Ok(None),
    }
}
/// Registered components in deterministic dependency order. Disabled components are absent.
#[cfg(test)]
pub fn component_ids(ctx: &VmContext) -> Vec<ComponentId> {
    let mut ids = ctx.core().components().get().map_or_else(Vec::new, VmComponents::ids);
    if let Some(components) = ctx.core().profiling().components() {
        for id in components.ids() { if !ids.contains(&id) { ids.push(id); } }
    }
    ids
}
