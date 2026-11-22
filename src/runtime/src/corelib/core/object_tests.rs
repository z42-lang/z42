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
        let h = obj_hash_code(&arr).unwrap();
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
    let a = obj_hash_code(&arr).unwrap();
    let b = obj_hash_code(&arr).unwrap();
    assert!(a == b);
}

#[test]
fn delegate_hash_matches_equality_shape() {
    // feat/delegate-object-methods：同一 FnId 的两个 FuncRef ⇒ 相等 ⇒ 同哈希；
    // 不同 FnId ⇒ 不等 ⇒（大概率）不同哈希；非委托 → 0（与 eq 的 lenient false 对称）。
    use crate::metadata::ids::tokens::FnId;
    let a = Value::FuncRef(FnId(7));
    let b = Value::FuncRef(FnId(7));
    let c = Value::FuncRef(FnId(9));
    assert!(delegate_eq(&a, &b));
    assert!(delegate_hash(&a) == delegate_hash(&b));
    assert!(!delegate_eq(&a, &c));
    assert!(delegate_hash(&a) != delegate_hash(&c));
    assert!(delegate_hash(&Value::I64(5)) == 0);
}
