//! Round-trip tests for [`super::marshal`].

use super::dispatch::SigType;
use super::marshal::{value_to_z42, z42_to_value, Arena};
use crate::metadata::Value;

fn arena() -> Arena { Arena::new() }

fn round_trip_int(v: i64, target: SigType) {
    let mut a = arena();
    let z = value_to_z42(&Value::I64(v), &target, &mut a).expect("encode");
    let back = z42_to_value(&z, &target).expect("decode");
    assert_eq!(back, Value::I64(v as i64), "i64 round-trip target={target:?}");
}

#[test]
fn i64_round_trip() {
    for v in [0, 1, -1, i64::MIN, i64::MAX, 0xDEAD_BEEF] {
        round_trip_int(v, SigType::I64);
    }
}

#[test]
fn i32_round_trip_truncates_via_libffi_layer_only() {
    // marshal layer keeps full i64 around; truncation is libffi's job at
    // the cif boundary. So a Value::I64 marshaled as i32 still round-trips
    // to the same Value::I64 here.
    let mut a = arena();
    let z = value_to_z42(&Value::I64(0x1234_5678_9ABC_DEF0_u64 as i64), &SigType::I32, &mut a).unwrap();
    let back = z42_to_value(&z, &SigType::I32).unwrap();
    assert_eq!(back, Value::I64(0x1234_5678_9ABC_DEF0_u64 as i64));
}

#[test]
fn f64_nan_round_trip() {
    let mut a = arena();
    let z = value_to_z42(&Value::F64(f64::NAN), &SigType::F64, &mut a).unwrap();
    let back = z42_to_value(&z, &SigType::F64).unwrap();
    let Value::F64(x) = back else { panic!("expected F64") };
    assert!(x.is_nan(), "NaN must survive round-trip");
}

#[test]
fn bool_round_trip() {
    let mut a = arena();
    for b in [true, false] {
        let z = value_to_z42(&Value::Bool(b), &SigType::Bool, &mut a).unwrap();
        let back = z42_to_value(&z, &SigType::Bool).unwrap();
        assert_eq!(back, Value::Bool(b));
    }
}

#[test]
fn null_into_pointer_target() {
    let mut a = arena();
    let z = value_to_z42(&Value::Null, &SigType::Ptr, &mut a).unwrap();
    assert_eq!(z.payload, 0);
}

#[test]
fn pointer_int_round_trip() {
    // Native pointers are stored as Value::I64 in C2 (typed wrapper lands in C5).
    let mut a = arena();
    let v = Value::I64(0x1000_2000_3000_4000_u64 as i64);
    let z = value_to_z42(&v, &SigType::Ptr, &mut a).unwrap();
    let back = z42_to_value(&z, &SigType::Ptr).unwrap();
    assert_eq!(back, v);
}

#[test]
fn unsupported_combo_errors() {
    // Char doesn't have a marshal path in C2 (covered when stdlib char ops
    // need to cross FFI; for now any non-i64/bool/f64/null is rejected).
    let mut a = arena();
    let err = value_to_z42(&Value::Char('z'), &SigType::I64, &mut a).expect_err("must reject");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("blittable") || msg.contains("Char"),
        "unexpected message: {msg}"
    );
}

// ── Spec C8: Str → CStr/Ptr marshal via Arena ───────────────────────────

#[test]
fn str_to_cstr_round_trip_payload_points_at_nul_terminated_buffer() {
    let mut a = arena();
    let z = value_to_z42(&Value::Str("hello".into()), &SigType::CStr, &mut a)
        .expect("Str → CStr ok");
    assert_eq!(a.temps_len(), 1, "arena now owns one CString");
    assert_ne!(z.payload, 0, "ptr non-null");

    // Read back through the raw pointer to verify NUL termination.
    let ptr = z.payload as *const std::os::raw::c_char;
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    assert_eq!(cstr.to_bytes(), b"hello");
}

#[test]
fn str_to_ptr_uses_same_path_as_cstr() {
    let mut a = arena();
    let z = value_to_z42(&Value::Str("z42".into()), &SigType::Ptr, &mut a).unwrap();
    let ptr = z.payload as *const std::os::raw::c_char;
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    assert_eq!(cstr.to_bytes(), b"z42");
}

#[test]
fn str_with_interior_nul_returns_invalid_marshal() {
    // retire-z-codes: interior NUL surfaces as
    // `MarshalErr::InvalidMarshal` carrying a plain-text diagnostic.
    // The caller (`exec_native::call_native`) wraps the message into a
    // `Std.InvalidMarshalException` instance for user-catchable throw.
    use crate::native::marshal::MarshalErr;
    let mut a = arena();
    let err = value_to_z42(&Value::Str("a\0b".into()), &SigType::CStr, &mut a)
        .expect_err("interior NUL rejected");
    match err {
        MarshalErr::InvalidMarshal(msg) => {
            assert!(msg.contains("interior NUL"), "msg = {msg}");
            assert!(!msg.contains("Z0908"), "Z prefix must be retired; msg = {msg}");
        }
        MarshalErr::Internal(e) => panic!("expected InvalidMarshal, got Internal: {e:#}"),
    }
}

#[test]
fn str_marshal_empty_string_is_just_a_nul_byte() {
    let mut a = arena();
    let z = value_to_z42(&Value::Str("".into()), &SigType::CStr, &mut a).unwrap();
    let ptr = z.payload as *const std::os::raw::c_char;
    let first = unsafe { *ptr };
    assert_eq!(first, 0, "empty CString starts with NUL");
}
