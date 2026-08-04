use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> { VmContext::new() }

fn extract_bytes(v: Value) -> Vec<u8> {
    match v {
        Value::Array(rc) => {
            let borrowed = rc.borrow();
            borrowed
                .iter_boxed()
                .map(|e| match e {
                    Value::I64(n) => {
                        assert!((0..=255).contains(&n), "byte out of range: {n}");
                        n as u8
                    }
                    other => panic!("expected I64 byte, got {other:?}"),
                })
                .collect()
        }
        other => panic!("expected Value::Array, got {other:?}"),
    }
}

// fix-crypto-tests-ctx-lifetime (2026-05-30): bind ctx to a named local
// so it outlives the returned Value::Array. The previous `&ctx()` inline
// pattern dropped the VmContext (and its owned heap region) at the end
// of the `let r = …;` statement, leaving `r`'s GcRef pointing into freed
// heap memory. Subsequent extract_bytes(r) dereferenced the dangling
// pointer — usually invisible on Linux/macOS (use-after-free reads stale
// data), reliably STATUS_ACCESS_VIOLATION (0xc0000005) on Windows.

#[test]
fn zero_length_returns_empty_array() {
    let c = ctx();
    let r = builtin_crypto_random_bytes(&c, &[Value::I64(0)]).unwrap();
    assert_eq!(extract_bytes(r).len(), 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn returns_requested_length() {
    let c = ctx();
    let r = builtin_crypto_random_bytes(&c, &[Value::I64(32)]).unwrap();
    assert_eq!(extract_bytes(r).len(), 32);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn two_calls_produce_different_bytes() {
    // 32 bytes = 256 bits of entropy; collision probability is ~2^-128.
    // If this test fails the CSPRNG is broken or the RNG seed is fixed.
    let c = ctx();
    let a = extract_bytes(builtin_crypto_random_bytes(&c, &[Value::I64(32)]).unwrap());
    let b = extract_bytes(builtin_crypto_random_bytes(&c, &[Value::I64(32)]).unwrap());
    assert_ne!(a, b, "two CSPRNG samples were byte-identical — RNG broken");
}

#[test]
fn negative_n_bails() {
    let c = ctx();
    let err = builtin_crypto_random_bytes(&c, &[Value::I64(-1)])
        .expect_err("expected bail on negative n");
    let msg = format!("{err:?}");
    assert!(msg.contains("non-negative"), "unexpected msg: {msg}");
}

#[test]
fn n_above_i32_max_bails() {
    let c = ctx();
    let n = (i32::MAX as i64) + 1;
    let err = builtin_crypto_random_bytes(&c, &[Value::I64(n)])
        .expect_err("expected bail on huge n");
    let msg = format!("{err:?}");
    assert!(msg.contains("exceeds"), "unexpected msg: {msg}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn distribution_byte_zero_does_not_dominate() {
    // Sanity check that the source isn't returning all-zero. 1024 bytes,
    // count zeros — under a true uniform u8 source, P(byte=0) = 1/256,
    // expected ~4 zeros. Threshold = 50 leaves astronomical margin.
    let c = ctx();
    let r = builtin_crypto_random_bytes(&c, &[Value::I64(1024)]).unwrap();
    let bytes = extract_bytes(r);
    let zeros = bytes.iter().filter(|b| **b == 0).count();
    assert!(zeros < 50, "{} zeros out of 1024 bytes — RNG suspect", zeros);
}
