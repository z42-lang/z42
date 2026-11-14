//! C embedding archive assembled from the runtime and independent backend rlibs.
pub use z42::host::*;

/// Register this archive's linked backends before calling the host C API.
#[no_mangle]
pub extern "C" fn z42_linked_components_init() -> i32 {
    match register() { Ok(()) => 0, Err(_) => 1 }
}
fn register() -> anyhow::Result<()> {
    unsafe { z42::host::register_static_component(z42::component_abi::INTERP_COMPONENT, z42_interp::component::z42_interp_init)?; }
    #[cfg(all(feature = "jit", z42_jit_supported))]
    unsafe { z42::host::register_static_component(z42::component_abi::JIT_COMPONENT, z42_jit::component::z42_jit_init)?; }
    Ok(())
}
