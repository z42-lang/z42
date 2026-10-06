//! Tests for the `Std.Object` builtins.

use super::*;

#[test]
fn identity_hash_low_bits_vary_across_objects() {
    // `Dictionary` takes the bucket from `h & mask`; with the raw address the
    // low 3 bits were always 0, so only 1 in 8 buckets could be a home slot.
    let ctx = VmContext::new();
    let mut low3 = [0usize; 8];
    for _ in 0..64 {
        let arr = ctx.heap().alloc_array(vec![Value::I64(0)]);
        let Value::I64(h) = builtin_obj_hash_code(&ctx, &[arr]).unwrap() else { panic!("hash must be I64") };
        assert!((0..=0x7fff_ffff).contains(&h), "hash must stay non-negative 31-bit: {h}");
        low3[(h & 7) as usize] += 1;
    }
    let used = low3.iter().filter(|&&n| n > 0).count();
    assert!(used >= 6, "low 3 bits hit only {used} of 8 values: {low3:?}");
}

#[test]
fn identity_hash_is_stable_for_one_object() {
    let ctx = VmContext::new();
    let arr = ctx.heap().alloc_array(Vec::new());
    let a = builtin_obj_hash_code(&ctx, &[arr]).unwrap();
    let b = builtin_obj_hash_code(&ctx, &[arr]).unwrap();
    assert!(matches!((a, b), (Value::I64(x), Value::I64(y)) if x == y));
}
