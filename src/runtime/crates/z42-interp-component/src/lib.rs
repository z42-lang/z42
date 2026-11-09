//! Dynamic runtime-pack export; ordinary workspace builds contain no backend here.
#[cfg(feature = "dynamic-component")]
#[no_mangle]
pub unsafe extern "C" fn z42_interp_init() -> *const backend::api::ComponentDescriptor {
    unsafe { backend::interp::component::z42_interp_init() }
}
