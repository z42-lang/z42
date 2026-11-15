//! Process-wide static factory selection. VM state belongs to component instances.
use crate::components::abi::{ComponentId, ComponentInit};
use anyhow::{ensure, Result};
use parking_lot::RwLock;
use std::{collections::BTreeMap, sync::OnceLock};

fn providers() -> &'static RwLock<BTreeMap<ComponentId, (ComponentInit, crate::loader::NativeOrigin)>> {
    static PROVIDERS: OnceLock<RwLock<BTreeMap<ComponentId, (ComponentInit, crate::loader::NativeOrigin)>>> = OnceLock::new();
    PROVIDERS.get_or_init(Default::default)
}

/// Make a statically linked component available to automatic backend selection.
/// Repeating the same registration is harmless; replacing a provider is rejected.
///
/// # Safety
/// The factory must implement the component protocol and remain valid until process exit.
pub unsafe fn register_static_component(id: ComponentId, init: ComponentInit) -> Result<()> {
    unsafe { register(id, init, crate::loader::NativeOrigin::StaticLink) }
}

/// Register a provider linked as a shared-library dependency.
/// # Safety
/// The provider and its shared library remain valid through process exit.
pub unsafe fn register_shared_component(id: ComponentId, init: ComponentInit) -> Result<()> {
    unsafe { register(id, init, crate::loader::NativeOrigin::SharedLink) }
}

unsafe fn register(id: ComponentId, init: ComponentInit, origin: crate::loader::NativeOrigin) -> Result<()> {
    let mut providers = providers().write();
    if let Some(previous) = providers.get(&id) {
        ensure!(std::ptr::fn_addr_eq(previous.0, init) && previous.1 == origin, "component {id} already has a different static provider");
    } else {
        providers.insert(id, (init, origin));
    }
    Ok(())
}

pub(super) fn find(id: ComponentId) -> Option<ComponentInit> {
    providers().read().get(&id).map(|p| p.0)
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;

pub(super) fn origin(id: ComponentId) -> crate::loader::NativeOrigin {
    providers().read().get(&id).map_or(crate::loader::NativeOrigin::StaticLink, |p| p.1)
}
