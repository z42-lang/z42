//! Dynamic runtime-pack export; ordinary workspace builds contain no backend here.
#[cfg(feature = "dynamic-component")]
#[no_mangle]
pub unsafe extern "C" fn z42_jit_init() -> *const backend::api::ComponentDescriptor {
    unsafe { backend::jit::component::z42_jit_init() }
}
