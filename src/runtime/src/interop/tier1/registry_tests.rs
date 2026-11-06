//! Unit tests for [`super::registry::RegisteredType`].

use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;

use z42_abi::{
    Z42MethodDesc, Z42TypeDescriptor_v1, Z42_ABI_VERSION, Z42_METHOD_FLAG_VIRTUAL,
};

use super::registry::RegisteredType;

extern "C" fn dummy_alloc() -> *mut c_void { Box::into_raw(Box::new([0u8; 8])) as *mut c_void }
extern "C" fn dummy_ctor(_self: *mut c_void, _args: *const z42_abi::Z42Args) {}
extern "C" fn dummy_dtor(_self: *mut c_void) {}
extern "C" fn dummy_dealloc(this: *mut c_void) {
    if !this.is_null() { unsafe { drop(Box::from_raw(this as *mut [u8; 8])) } }
}
extern "C" fn dummy_inc(_self: *mut c_void) -> i64 { 42 }

/// Holder so the descriptor's `*const c_char` strings stay alive for the
/// duration of the test.
struct DescStorage {
    module:    CString,
    type_name: CString,
    method_name:    CString,
    method_sig:     CString,
    methods: Vec<Z42MethodDesc>,
}

fn build_descriptor() -> (Z42TypeDescriptor_v1, Box<DescStorage>) {
    let mut storage = Box::new(DescStorage {
        module:    CString::new("tests").unwrap(),
        type_name: CString::new("Probe").unwrap(),
        method_name: CString::new("inc").unwrap(),
        method_sig:  CString::new("(*mut Self) -> i64").unwrap(),
        methods: Vec::new(),
    });
    storage.methods.push(Z42MethodDesc {
        name: storage.method_name.as_ptr(),
        signature: storage.method_sig.as_ptr(),
        fn_ptr: dummy_inc as *mut c_void,
        flags: Z42_METHOD_FLAG_VIRTUAL,
        reserved: 0,
    });
    let desc = Z42TypeDescriptor_v1 {
        abi_version:    Z42_ABI_VERSION,
        flags:          0,
        module_name:    storage.module.as_ptr(),
        type_name:      storage.type_name.as_ptr(),
        instance_size:  8,
        instance_align: 8,
        alloc:    Some(dummy_alloc),
        ctor:     Some(dummy_ctor),
        dtor:     Some(dummy_dtor),
        dealloc:  Some(dummy_dealloc),
        retain:   None,
        release:  None,
        method_count: storage.methods.len(),
        methods:      storage.methods.as_ptr(),
        field_count:  0,
        fields:       ptr::null(),
        trait_impl_count: 0,
        trait_impls:      ptr::null(),
    };
    (desc, storage)
}

#[test]
fn from_descriptor_reads_name_and_methods() {
    let (desc, _storage) = build_descriptor();
    let ty = unsafe { RegisteredType::from_descriptor(&desc) }.expect("registers");
    assert_eq!(ty.module(), "tests");
    assert_eq!(ty.type_name(), "Probe");
    assert_eq!(ty.method_count(), 1);
    let m = ty.method("inc").expect("inc method present");
    assert_eq!(m.signature, "(*mut Self) -> i64");
    assert!(!m.fn_ptr.is_null());
}

// retire-z-codes: error messages no longer carry `Z####:`
// prefixes; assertions match the diagnostic text body directly.

#[test]
fn rejects_null_descriptor() {
    let err = unsafe { RegisteredType::from_descriptor(ptr::null()) }.expect_err("null rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("null descriptor"), "msg = {msg}");
    assert!(!msg.contains("Z0905"), "Z prefix must be retired; msg = {msg}");
}

#[test]
fn rejects_wrong_abi_version() {
    let (mut desc, _storage) = build_descriptor();
    desc.abi_version = 2; // future-version sentinel
    let err = unsafe { RegisteredType::from_descriptor(&desc) }.expect_err("rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("ABI version mismatch"), "msg = {msg}");
    assert!(!msg.contains("Z0906"), "Z prefix must be retired; msg = {msg}");
}

#[test]
fn rejects_null_module_name() {
    let (mut desc, _storage) = build_descriptor();
    desc.module_name = ptr::null();
    let err = unsafe { RegisteredType::from_descriptor(&desc) }.expect_err("rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("module_name"), "msg = {msg}");
    assert!(!msg.contains("Z0905"), "Z prefix must be retired; msg = {msg}");
}

#[test]
fn rejects_unsupported_signature() {
    let (mut desc, mut storage) = build_descriptor();
    storage.method_sig = CString::new("(&[i64]) -> i64").unwrap();
    storage.methods[0].signature = storage.method_sig.as_ptr();
    desc.methods = storage.methods.as_ptr();
    let err = unsafe { RegisteredType::from_descriptor(&desc) }.expect_err("rejected");
    assert!(format!("{err:#}").contains("unsupported signature"));
}

#[test]
fn rejects_method_count_with_null_methods() {
    let (mut desc, _storage) = build_descriptor();
    desc.method_count = 1;
    desc.methods = ptr::null();
    let err = unsafe { RegisteredType::from_descriptor(&desc) }.expect_err("rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("methods array is null"), "msg = {msg}");
    assert!(!msg.contains("Z0905"), "Z prefix must be retired; msg = {msg}");
}
