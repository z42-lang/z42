//! Process-wide static factory selection. VM state belongs to component instances.
use super::{ComponentId, ComponentInit};
use anyhow::{ensure, Result};
use parking_lot::RwLock;
use std::{collections::BTreeMap, sync::OnceLock};

fn providers() -> &'static RwLock<BTreeMap<ComponentId, ComponentInit>> {
    static PROVIDERS: OnceLock<RwLock<BTreeMap<ComponentId, ComponentInit>>> = OnceLock::new();
    PROVIDERS.get_or_init(Default::default)
}

/// Make a statically linked component available to automatic backend selection.
/// Repeating the same registration is harmless; replacing a provider is rejected.
///
/// # Safety
/// The factory must implement the component protocol and remain valid until process exit.
pub unsafe fn register_static_component(id: ComponentId, init: ComponentInit) -> Result<()> {
    let mut providers = providers().write();
    if let Some(previous) = providers.get(&id) {
        ensure!(std::ptr::fn_addr_eq(*previous, init), "component {id} already has a different static provider");
    } else {
        providers.insert(id, init);
    }
    Ok(())
}

pub(super) fn find(id: ComponentId) -> Option<ComponentInit> {
    providers().read().get(&id).copied()
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;
