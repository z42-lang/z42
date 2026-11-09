use super::api::*;
#[test]
fn wire_headers_and_ids_have_fixed_shapes() {
    assert_eq!(core::mem::size_of::<ComponentId>(), 16);
    assert_eq!(core::mem::size_of::<InterfaceId>(), 16);
    assert_eq!(core::mem::size_of::<ComponentStatus>(), 4);
    assert_eq!(core::mem::size_of::<ApiHeader>(), 8);
    assert_eq!(core::mem::offset_of!(ComponentDescriptor, header), 0);
    assert_eq!(core::mem::offset_of!(VmInfoApi, header), 0);
    assert_eq!(
        InterfaceSpec::of::<VmInfoApi>().size as usize,
        core::mem::size_of::<VmInfoApi>()
    );
    assert_eq!(ComponentId::new(1).0[15], 1);
}
#[test]
fn unknown_status_values_are_representable() {
    assert_ne!(ComponentStatus(u32::MAX), ComponentStatus::OK);
    assert_ne!(COMPONENT_BUILD_ID, [0; 32]);
}
