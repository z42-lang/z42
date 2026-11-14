use super::*;
unsafe extern "C" fn first() -> *const crate::components::abi::ComponentDescriptor { std::ptr::null() }
unsafe extern "C" fn second() -> *const crate::components::abi::ComponentDescriptor { std::ptr::null() }
#[test]
fn static_providers_are_idempotent_and_conflicting_replacement_is_rejected() {
    let id = ComponentId::new(0x8991795687984578374);
    assert!(find(id).is_none());
    unsafe {
        register_static_component(id, first).unwrap();
        register_static_component(id, first).unwrap();
        assert!(register_static_component(id, second).unwrap_err().to_string().contains("different static provider"));
    }
    assert!(std::ptr::fn_addr_eq(find(id).unwrap(), first as ComponentInit));
}
