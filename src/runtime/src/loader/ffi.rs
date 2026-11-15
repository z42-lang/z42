//! Native final-link registration, independent of host sessions and error state.
use std::ffi::{c_char, CStr};
use z42_abi::module::{OK, ERROR};
use super::{linked::NativeSymbolResolver, NativeOrigin};

unsafe fn register(name: *const c_char, resolve: Option<NativeSymbolResolver>, origin: NativeOrigin) -> u32 {
    let Some(resolve) = resolve else { return ERROR; };
    if name.is_null() { return ERROR; }
    let Ok(name) = (unsafe { CStr::from_ptr(name) }).to_str() else { return ERROR; };
    match std::panic::catch_unwind(|| unsafe {
        match origin {
            NativeOrigin::StaticLink => super::linked::register_static_library(name, resolve),
            NativeOrigin::SharedLink => super::linked::register_shared_library(name, resolve),
            NativeOrigin::RuntimeLoad => unreachable!(),
        }
    }) {
        Ok(Ok(())) => OK,
        _ => ERROR,
    }
}
/// # Safety
/// Name is NUL-terminated UTF-8; resolver/code are trusted and process-lifetime.
#[no_mangle]
pub unsafe extern "C" fn z42_native_register_static(name: *const c_char, resolve: Option<NativeSymbolResolver>) -> u32 {
    unsafe { register(name, resolve, NativeOrigin::StaticLink) }
}
/// # Safety
/// Same contract as static registration; code is supplied by a shared-library link.
#[no_mangle]
pub unsafe extern "C" fn z42_native_register_shared(name: *const c_char, resolve: Option<NativeSymbolResolver>) -> u32 {
    unsafe { register(name, resolve, NativeOrigin::SharedLink) }
}

#[cfg(test)]
#[path = "ffi_tests.rs"]
mod tests;
