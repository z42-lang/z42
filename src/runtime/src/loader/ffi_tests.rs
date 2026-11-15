use super::*;
#[test]
fn native_registration_is_lazy_and_does_not_touch_host_errors() {
    unsafe extern "C" fn resolver(_: *const c_char) -> *const std::ffi::c_void { panic!("registration must not resolve symbols") }
    crate::host::error::set_error(crate::host::error::Z42HostStatus::BadConfig, "sentinel");
    let before = crate::host::error::snapshot_last_error();
    unsafe {
        assert_eq!(z42_native_register_static(std::ptr::null(), Some(resolver)), ERROR);
        assert_eq!(z42_native_register_static(c"loader.ffi.test".as_ptr(), None), ERROR);
        assert_eq!(z42_native_register_static(c"loader.ffi.test".as_ptr(), Some(resolver)), OK);
        assert_eq!(z42_native_register_static(c"loader.ffi.test".as_ptr(), Some(resolver)), OK);
        assert_eq!(z42_native_register_shared(c"loader.ffi.test".as_ptr(), Some(resolver)), ERROR);
        assert_eq!(z42_native_register_shared(c"loader.ffi.shared.test".as_ptr(), Some(resolver)), OK);
    }
    assert_eq!(crate::host::error::snapshot_last_error().message, before.message);
    crate::host::error::clear_error();
}
