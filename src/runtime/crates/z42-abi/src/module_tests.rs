use super::*;
use core::mem::{offset_of, size_of};

#[test]
fn descriptor_layout_keeps_version_prefix_and_c_pointer_fields() {
    let pointer = size_of::<*const ()>();
    assert_eq!(size_of::<Text>(), 2 * pointer);
    assert_eq!(size_of::<Parameters>(), 2 * pointer);
    assert_eq!(size_of::<Call>(), 3 * pointer);
    assert_eq!(offset_of!(Module, version), 0);
    assert_eq!(offset_of!(Module, size), 4);
    assert_eq!(offset_of!(Module, name), 8);
    assert_eq!(size_of::<Module>(), 8 + 6 * pointer);
    assert_eq!(offset_of!(Function, result), 4 * pointer);
    assert_eq!(offset_of!(Function, invoke), 4 * pointer + 8);
    assert_eq!(offset_of!(HostApi, capabilities), 8);
    assert_eq!(offset_of!(HostApi, argument), 16);
    assert_eq!(offset_of!(Value, bits), 8);
    assert_eq!(offset_of!(Value, data), 16);
    assert_eq!(size_of::<Option<Invoke>>(), pointer);
    assert_ne!(VOID, NULL);
}
