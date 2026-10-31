//! In-process libffi dispatch tests — exercise [`super::dispatch`] without
//! involving `dlopen` or any external native library.

use std::os::raw::c_void;

use super::dispatch::{
    self, build_cif, parse_signature, SigType, Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64,
    Z42_VALUE_TAG_I64, Z42_VALUE_TAG_NATIVEPTR, Z42_VALUE_TAG_NULL,
};
use z42_abi::Z42Value;

extern "C" fn add_i64(a: i64, b: i64) -> i64 { a + b }
extern "C" fn add_i32(a: i32, b: i32) -> i32 { a + b }
extern "C" fn neg_f64(x: f64) -> f64 { -x }
extern "C" fn is_pos(x: i64) -> u8 { if x > 0 { 1 } else { 0 } }
extern "C" fn no_args() -> i64 { 7 }
extern "C" fn returns_void(_: i64) {}
extern "C" fn returns_ptr(x: i64) -> *mut c_void { x as usize as *mut c_void }

fn z42_i64(v: i64) -> Z42Value {
    Z42Value { tag: Z42_VALUE_TAG_I64, reserved: 0, payload: v as u64 }
}

#[test]
fn parse_signature_basic_forms() {
    let (params, ret) = parse_signature("(i64, i64) -> i64").unwrap();
    assert_eq!(params, vec![SigType::I64, SigType::I64]);
    assert_eq!(ret, SigType::I64);

    let (params, ret) = parse_signature("() -> ()").unwrap();
    assert!(params.is_empty());
    assert_eq!(ret, SigType::Void);

    let (params, ret) = parse_signature("(*mut Self) -> i64").unwrap();
    assert_eq!(params, vec![SigType::SelfRef]);
    assert_eq!(ret, SigType::I64);
}

#[test]
fn parse_signature_rejects_unsupported_types() {
    let err = parse_signature("(&[i64]) -> i64").expect_err("must reject");
    assert!(format!("{err:#}").contains("unsupported"));
}

#[test]
fn add_i64_via_libffi() {
    let cif = build_cif(&[SigType::I64, SigType::I64], &SigType::I64);
    let args = [z42_i64(3), z42_i64(4)];
    let result = unsafe {
        dispatch::call(&cif, add_i64 as *mut c_void, &args, &[SigType::I64, SigType::I64], &SigType::I64)
    }
    .unwrap();
    assert_eq!(result.tag, Z42_VALUE_TAG_I64);
    assert_eq!(result.payload, 7);
}

#[test]
fn add_i32_truncates_correctly() {
    let cif = build_cif(&[SigType::I32, SigType::I32], &SigType::I32);
    // pass 0xFFFF_FFFE + 0x1 — fits as i32 = -1; expect 0 (-1 + 1 = 0)
    let args = [z42_i64(-2_i64), z42_i64(1_i64)];
    let result = unsafe {
        dispatch::call(&cif, add_i32 as *mut c_void, &args, &[SigType::I32, SigType::I32], &SigType::I32)
    }
    .unwrap();
    assert_eq!(result.tag, Z42_VALUE_TAG_I64);
    assert_eq!(result.payload as i64, -1);
}

#[test]
fn neg_f64_via_libffi() {
    let cif = build_cif(&[SigType::F64], &SigType::F64);
    let mut args = [Z42Value { tag: Z42_VALUE_TAG_F64, reserved: 0, payload: 3.5_f64.to_bits() }];
    let result = unsafe {
        dispatch::call(&cif, neg_f64 as *mut c_void, &mut args, &[SigType::F64], &SigType::F64)
    }
    .unwrap();
    assert_eq!(result.tag, Z42_VALUE_TAG_F64);
    assert_eq!(f64::from_bits(result.payload), -3.5);
}

#[test]
fn bool_return_normalised() {
    let cif = build_cif(&[SigType::I64], &SigType::Bool);
    let args = [z42_i64(5)];
    let r = unsafe {
        dispatch::call(&cif, is_pos as *mut c_void, &args, &[SigType::I64], &SigType::Bool)
    }
    .unwrap();
    assert_eq!(r.tag, Z42_VALUE_TAG_BOOL);
    assert_eq!(r.payload, 1);

    let args = [z42_i64(-1)];
    let r = unsafe {
        dispatch::call(&cif, is_pos as *mut c_void, &args, &[SigType::I64], &SigType::Bool)
    }
    .unwrap();
    assert_eq!(r.tag, Z42_VALUE_TAG_BOOL);
    assert_eq!(r.payload, 0);
}

#[test]
fn no_args_call() {
    let cif = build_cif(&[], &SigType::I64);
    let r = unsafe { dispatch::call(&cif, no_args as *mut c_void, &[], &[], &SigType::I64) }.unwrap();
    assert_eq!(r.payload, 7);
}

#[test]
fn void_return_yields_null_tag() {
    let cif = build_cif(&[SigType::I64], &SigType::Void);
    let args = [z42_i64(1)];
    let r = unsafe {
        dispatch::call(&cif, returns_void as *mut c_void, &args, &[SigType::I64], &SigType::Void)
    }
    .unwrap();
    assert_eq!(r.tag, Z42_VALUE_TAG_NULL);
}

#[test]
fn pointer_return_yields_native_ptr_tag() {
    let cif = build_cif(&[SigType::I64], &SigType::Ptr);
    let args = [z42_i64(0xDEAD_BEEF)];
    let r = unsafe {
        dispatch::call(&cif, returns_ptr as *mut c_void, &args, &[SigType::I64], &SigType::Ptr)
    }
    .unwrap();
    assert_eq!(r.tag, Z42_VALUE_TAG_NATIVEPTR);
    assert_eq!(r.payload, 0xDEAD_BEEF);
}

#[test]
fn arg_count_mismatch_errors() {
    let cif = build_cif(&[SigType::I64, SigType::I64], &SigType::I64);
    let err = unsafe {
        dispatch::call(&cif, add_i64 as *mut c_void, &[z42_i64(1)], &[SigType::I64, SigType::I64], &SigType::I64)
    }
    .expect_err("argument count mismatch");
    assert!(format!("{err:#}").contains("argument count"));
}
