//! add-value-hash-ref-leaves：引用叶子的哈希贡献——object/array 走地址身份哈希（此前常量 0，
//! 引用叶子互异的 struct 全落同一桶），string 走内容，null/委托保持常量。

use super::*;
use crate::gc::Heap;
use crate::object::{NativeData, TypeDesc};
use std::sync::Arc;

fn plain_td(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.to_string(), base_name: None,
        fields: Vec::new(), field_index: crate::zpkg::NameIndex::new(),
        vtable: Vec::new(), vtable_index: crate::zpkg::NameIndex::new(),
        cold: None, id: crate::metadata::ids::tokens::TypeId::UNRESOLVED,
    })
}

fn new_object(heap: &Heap) -> Value {
    match heap.alloc_object(plain_td("Demo.Cell"), Vec::new(), NativeData::None) {
        Value::Object(gc) => Value::Object(gc),
        other => panic!("expected a fresh object, got {other:?}"),
    }
}

#[test]
fn object_leaf_hash_separates_distinct_objects() {
    let heap = Heap::default();
    let a = new_object(&heap);
    let b = new_object(&heap);
    // 此前两者贡献同为 0 ⇒ 哈希相同（全落同一桶）；现在按地址身份散布。
    // fmix64 混合下碰撞概率 2^-31，与 obj_hash_code 同底。
    assert_ne!(hash_struct_payload(&[], &[a.clone()]), hash_struct_payload(&[], &[b]));
}

#[test]
fn object_leaf_hash_is_stable_and_contract_holds() {
    let heap = Heap::default();
    let a = new_object(&heap);
    let h1 = hash_struct_payload(&[1, 2], &[a.clone()]);
    let h2 = hash_struct_payload(&[1, 2], &[a.clone()]);   // 同一引用（同值 struct）⇒ 同哈希
    assert_eq!(h1, h2);
    assert!(h1 >= 0);                                       // 非负（Dictionary 契约）
}

#[test]
fn string_leaf_hashes_content_null_leaf_contributes_nothing() {
    let s1 = Value::Str("a".into());
    let s2 = Value::Str("a".into());
    assert_eq!(hash_struct_payload(&[], &[s1]), hash_struct_payload(&[], &[s2]));   // 内容相等 ⇒ 同哈希
    assert_ne!(hash_struct_payload(&[], &[Value::Str("a".into())]),
               hash_struct_payload(&[], &[Value::Str("b".into())]));
    // null 叶子是常量贡献：FNV 对每个叶子都有一步乘法进位（xor 0 仍乘 prime）⇒ 有 null 叶子
    // 与无叶子哈希不同，但同形状同 null 的两个 struct 哈希相同——null 相等语义下契约成立。
    assert_ne!(hash_struct_payload(&[], &[Value::Null]), hash_struct_payload(&[], &[]));
    assert_eq!(hash_struct_payload(&[1], &[Value::Null]), hash_struct_payload(&[1], &[Value::Null]));
}

#[test]
fn borrowed_float_hash_matches_normalized_snapshot() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&i32::MIN.to_le_bytes());
    bytes.extend_from_slice(&(-0.0_f32).to_le_bytes());
    bytes.extend_from_slice(&(-2.5_f64).to_le_bytes());
    bytes.extend_from_slice(&(-0.0_f64).to_le_bytes());
    bytes.push(17);
    let ranges = [4..8, 8..16, 16..24];
    let refs = [Value::Str("field".into()), Value::Null];
    let mut normalized = bytes.clone();
    normalize_signed_zero(&mut normalized, &ranges).unwrap();

    assert_eq!(normalized[7], 0);
    assert_eq!(normalized[15], 0xc0);
    assert_eq!(normalized[23], 0);
    assert_eq!(hash_struct_with_floats(&bytes, &refs, &ranges).unwrap(),
               hash_struct_payload(&normalized, &refs));
}
