//! Final-link composition ABI. It has no dependency on host sessions or errors.
use super::abi::{ComponentId, ComponentInit, ComponentStatus};

/// Register a trusted process-lifetime factory without invoking it.
///
/// # Safety
/// The factory and its descriptor/callbacks must obey the same-build protocol
/// and remain valid until process exit.
#[no_mangle]
pub unsafe extern "C" fn z42_component_register_static(
    id: ComponentId, init: Option<ComponentInit>,
) -> ComponentStatus {
    let Some(init) = init else { return ComponentStatus::INVALID_ARGUMENT; };
    match std::panic::catch_unwind(|| unsafe { super::linked::register_static_component(id, init) }) {
        Ok(Ok(())) => ComponentStatus::OK,
        Ok(Err(_)) | Err(_) => ComponentStatus::ERROR,
    }
}

/// Final static archives provide their selected composition. A dynamic Core
/// discovers providers during preparation and has no linked defaults.
#[cfg(not(feature = "static-embedding"))]
#[no_mangle]
pub extern "C" fn z42_linked_components_init() -> i32 { 0 }

#[cfg(test)]
#[path = "ffi_tests.rs"]
mod tests;
