use super::{pin_owned_buffer, release_owned_buffer, pinned_owned_buffer_count};
use crate::vm_context::VmContext;

#[test]
fn pinned_buffers_are_isolated_and_release_is_idempotent() {
    let first = VmContext::new();
    let second = VmContext::new();
    let ptr = pin_owned_buffer(&first, vec![7_u8, 8, 9].into_boxed_slice()).unwrap();
    assert_eq!(pinned_owned_buffer_count(&first).unwrap(), 1);
    assert_eq!(pinned_owned_buffer_count(&second).unwrap(), 0);
    // The owned snapshot retains its address and bytes while the VM holds it.
    assert_eq!(unsafe { std::slice::from_raw_parts(ptr as *const u8, 3) }, &[7, 8, 9]);
    release_owned_buffer(&second, ptr).unwrap();
    assert_eq!(pinned_owned_buffer_count(&first).unwrap(), 1);
    release_owned_buffer(&first, ptr).unwrap();
    release_owned_buffer(&first, ptr).unwrap();
    assert_eq!(pinned_owned_buffer_count(&first).unwrap(), 0);
}

#[test]
fn worker_can_release_a_parent_buffer_and_core_keeps_state_alive() {
    let parent = VmContext::new();
    let ptr = pin_owned_buffer(&parent, vec![1_u8].into_boxed_slice()).unwrap();
    let core = parent.core_arc();
    drop(parent);
    std::thread::spawn(move || {
        let worker = VmContext::new_with_core(core);
        assert_eq!(pinned_owned_buffer_count(&worker).unwrap(), 1);
        release_owned_buffer(&worker, ptr).unwrap();
        assert_eq!(pinned_owned_buffer_count(&worker).unwrap(), 0);
    }).join().unwrap();
}

#[cfg(feature = "native-interop")]
fn descriptor() -> Box<z42_abi::Z42TypeDescriptor_v1> {
    Box::new(z42_abi::Z42TypeDescriptor_v1 {
        abi_version: z42_abi::Z42_ABI_VERSION, flags: 0,
        module_name: b"state-test\0".as_ptr().cast(), type_name: b"Probe\0".as_ptr().cast(),
        instance_size: 0, instance_align: 1,
        alloc: None, ctor: None, dtor: None, dealloc: None, retain: None, release: None,
        method_count: 0, methods: std::ptr::null(), field_count: 0, fields: std::ptr::null(),
        trait_impl_count: 0, trait_impls: std::ptr::null(),
    })
}

#[cfg(feature = "native-interop")]
#[test]
fn registered_types_share_with_workers_preserve_duplicates_and_isolate_vms() {
    use super::{register_native_type, resolve_native_type, native_type_count, RegisteredType};
    use std::sync::Arc;
    let desc = descriptor();
    let ctx = VmContext::new();
    let other = VmContext::new();
    let first = Arc::new(unsafe { RegisteredType::from_descriptor(&*desc) }.unwrap());
    let duplicate = Arc::new(unsafe { RegisteredType::from_descriptor(&*desc) }.unwrap());
    assert!(register_native_type(&ctx, first.clone()).unwrap());
    assert!(!register_native_type(&ctx, duplicate).unwrap());
    assert_eq!(native_type_count(&ctx).unwrap(), 1);
    assert!(Arc::ptr_eq(&resolve_native_type(&ctx, "state-test", "Probe").unwrap().unwrap(), &first));
    assert!(resolve_native_type(&other, "state-test", "Probe").unwrap().is_none());
    let core = ctx.core_arc();
    std::thread::spawn(move || {
        let worker = VmContext::new_with_core(core);
        assert!(Arc::ptr_eq(&resolve_native_type(&worker, "state-test", "Probe").unwrap().unwrap(), &first));
    }).join().unwrap();
}

#[cfg(feature = "native-interop")]
#[test]
fn c_abi_duplicate_registration_retains_its_error_contract() {
    let desc = descriptor();
    let ctx = VmContext::new();
    let _guard = super::exports::VmGuard::enter(&ctx);
    let first = unsafe { super::exports::z42_register_type(&*desc) };
    assert!(!first.0.is_null());
    let duplicate = unsafe { super::exports::z42_register_type(&*desc) };
    assert!(duplicate.0.is_null());
    let error = super::error::last();
    assert_eq!(error.code, 905);
    assert_eq!(super::native_type_count(&ctx).unwrap(), 1);
}
