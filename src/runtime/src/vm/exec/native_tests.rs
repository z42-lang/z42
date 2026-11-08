use super::services;
use crate::vm::context::VmContext;

#[test]
fn native_services_and_owned_pins_are_shared_with_workers() {
    let primary = VmContext::new();
    let worker = VmContext::new_with_core(primary.core_arc());
    assert!(std::ptr::eq(services(&primary).unwrap(), services(&worker).unwrap()));
    let ptr = (services(&primary).unwrap().pin_owned_buffer)(&primary, vec![1, 2, 3].into_boxed_slice()).unwrap();
    assert_eq!(crate::interop::pinned_owned_buffer_count(&worker).unwrap(), 1);
    drop(primary);
    (services(&worker).unwrap().release_owned_buffer)(&worker, ptr).unwrap();
    assert_eq!(crate::interop::pinned_owned_buffer_count(&worker).unwrap(), 0);
    (services(&worker).unwrap().release_owned_buffer)(&worker, ptr).unwrap();
    assert_eq!(crate::interop::pinned_owned_buffer_count(&worker).unwrap(), 0);
}
