//! Unit tests for fix-generic-array-value-zero-init (方案 C): `array_new` resolving a
//! generic type-param element to a concrete type via `frame.method_type_args` /
//! receiver `type_args`, so value-type slots get the type's zero instead of Null.
use super::*;
use crate::metadata::bytecode::Module;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn empty_module() -> Module {
    Module {
        name: "test".to_owned(),
        string_pool: vec![],
        classes: vec![],
        functions: vec![],
        type_registry: rustc_hash::FxHashMap::default(),
        type_registry_vec: Vec::new(),
        func_index: rustc_hash::FxHashMap::default(),
        func_ref_cache_slots: 0,
    }
}

// TAG_UNKNOWN (0x00): what codegen emits for an erased generic-param element.
const TAG_UNKNOWN: u8 = 0x00;

/// kind=1 (method-level) + method_type_args=["int"] → unwritten slots are I64(0),
/// not Null. This is the core of the bug fix.
#[test]
fn method_level_generic_int_array_zero_inits() {
    let ctx = VmContext::new();
    let module = empty_module();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::I64(3)); // reg0 = size
    frame.method_type_args = vec!["int".to_string()].into_boxed_slice();

    // new T[3] where T is method-level type param #0 (kind=1).
    let thrown = array_new(&ctx, &module, &mut frame, 1, 0, TAG_UNKNOWN, "T", false, 1, 0).unwrap();
    assert!(thrown.is_none(), "no OOM expected");

    frame.set(2, Value::I64(0)); // idx reg
    array_get(&ctx, &mut frame, 3, 1, 2).unwrap();
    assert!(
        matches!(frame.get(3).unwrap(), Value::I64(0)),
        "unwritten int slot must be I64(0), got {:?}",
        frame.get(3)
    );
}

/// kind=1 resolving to a reference type (string) → slots stay Null (no regression).
#[test]
fn method_level_generic_ref_array_stays_null() {
    let ctx = VmContext::new();
    let module = empty_module();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::I64(2));
    frame.method_type_args = vec!["string".to_string()].into_boxed_slice();

    array_new(&ctx, &module, &mut frame, 1, 0, TAG_UNKNOWN, "T", false, 1, 0).unwrap();
    frame.set(2, Value::I64(0));
    array_get(&ctx, &mut frame, 3, 1, 2).unwrap();
    assert!(matches!(frame.get(3).unwrap(), Value::Null), "string element default is Null");
}

/// kind=0 (non-generic) keeps the original default_value_for_tag path unchanged:
/// an Unknown tag with no resolution yields Null (pre-fix behavior preserved).
#[test]
fn non_generic_unknown_tag_unchanged() {
    let ctx = VmContext::new();
    let module = empty_module();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::I64(2));
    // kind=0, index=-1: no generic resolution, no method_type_args consulted.
    array_new(&ctx, &module, &mut frame, 1, 0, TAG_UNKNOWN, "T", false, 0, -1).unwrap();
    frame.set(2, Value::I64(0));
    array_get(&ctx, &mut frame, 3, 1, 2).unwrap();
    assert!(matches!(frame.get(3).unwrap(), Value::Null), "kind=0 Unknown tag → Null (unchanged)");
}

/// kind=1 but out-of-range index / empty method_type_args → graceful Null (no panic).
#[test]
fn method_level_oob_index_graceful_null() {
    let ctx = VmContext::new();
    let module = empty_module();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::I64(1));
    // method_type_args empty, but kind=1 index=0 → get(0) is None → falls back to tag.
    array_new(&ctx, &module, &mut frame, 1, 0, TAG_UNKNOWN, "T", false, 1, 0).unwrap();
    frame.set(2, Value::I64(0));
    array_get(&ctx, &mut frame, 3, 1, 2).unwrap();
    assert!(matches!(frame.get(3).unwrap(), Value::Null), "OOB type-arg → Null, no panic");
}
