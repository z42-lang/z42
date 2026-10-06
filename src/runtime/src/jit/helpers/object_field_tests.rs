//! fix-silent-prim-field-write：JIT 字段写 helper 的单测。
//!
//! **为什么不能只靠 e2e golden**（与 `array_tests.rs` 抬头同一条教训）：要让 `jit_field_set`
//! 真被执行，得让那个做 `FieldSet` 的函数**真被 JIT 编译**。实测本刀的 e2e fixture
//! （`src/tests/reflection/prim_field_null_write`）在 `--mode jit` 下 `Z42_JIT_PROFILE=1`
//! 只编了 9 个函数，属性 setter `Holder.set_P` **不在其中** —— 那几行 "threw" 其实是
//! 解释器跑的。⇒ e2e 覆盖的是 interp 那半，JIT 这半只有直接打 helper 才是确定性的。
//!
//! 被测行为：往**基元**字段写 `Value::Null` 必须置异常并返回 1（此前
//! `let _ = encode_prim(..)` 把错误丢了 ⇒ 静默无效）。配套两条对照，确保不是「把整条路堵死」。

use super::*;
use crate::jit::helpers::take_exception;
use super::super::super::frame::{JitFrame, JitModuleCtx};
use crate::metadata::types::{
    FieldAccess, ObjStorage, ObjectLayout, ScriptObject, TypeDesc, TypeDescCold, Value,
    STRUCT_LEAF_PRIM, TAG_I32, TAG_STR,
};
use crate::gc::GcRef;
use crate::vm_context::VmContext;
use std::sync::Arc;

/// 最小 JIT ctx：只有 `vm_ctx` 是活的（module 悬空——本组 helper 不碰 module）。
/// 手法同 `array_tests.rs::make_jit_ctx`。
fn make_jit_ctx(vm_ctx: &VmContext) -> JitModuleCtx {
    JitModuleCtx {
        fn_entries_by_id: Vec::new(),
        module:           std::ptr::null(),
        lazy:             std::ptr::null(),
        merged_len:       0,
        lazy_table:       std::sync::Mutex::new(crate::jit::frame::LazyTable::default()),
        vm_ctx:           vm_ctx as *const VmContext as *mut VmContext,
        call_counts:      Vec::new(),
        jit_threshold:    1,
        osr_entries:      std::sync::Mutex::new(std::collections::HashMap::new()),
        osr_threshold:    10_000,
        stack_limit: 0,
    }
}

/// `class Holder { int n; string s; }`
/// —— `n` 是字节打包的基元叶子（offset 0, 4B, TAG_I32）；
///    `s` 是 refs 侧表里的引用叶子（ref_slot 0）。
fn holder() -> Value {
    let layout = Arc::new(ObjectLayout {
        size: 4,
        field_offsets: Box::new([0, 0]),
        field_sizes:   Box::new([4, 8]),
        field_kinds:   Box::new([STRUCT_LEAF_PRIM, STRUCT_LEAF_PRIM]),
        ref_offsets:   Box::new([0]),
        ref_kinds:     Box::new([TAG_STR]),
        inline_refs:   Box::new([]),
        field_access:  Box::new([
            FieldAccess { offset: 0, width: 4, tag: TAG_I32, ref_slot: -1 },
            FieldAccess { offset: 0, width: 8, tag: TAG_STR, ref_slot: 0 },
        ]),
    });
    let mut field_index = crate::metadata::NameIndex::new();
    field_index.insert("n".to_string(), 0);
    field_index.insert("s".to_string(), 1);
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "Holder".to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold {
            composed_object_layout: Some(layout),
            ..Default::default()
        })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    Value::Object(GcRef::new(ScriptObject::new(td, ObjStorage::new(4, 1))))
}

/// 读回字段（`n` 是基元 ⇒ 从 bytes 解码）。
fn read_n(v: &Value) -> Value {
    match v {
        Value::Object(rc) => rc.borrow().field_value(0),
        _ => unreachable!("holder() 造的是堆对象"),
    }
}

fn set(frame: &mut JitFrame, ctx: &JitModuleCtx, name: &str) -> u8 {
    unsafe { jit_field_set(frame, ctx, 0, name.as_ptr(), name.len(), 1, std::ptr::null()) }
}

/// 🔴 正例：`Null` 写进 `int` 字段必须置异常 + 返回 1，且**不得改动字段**。
///
/// 回归前：`set_field_value` 的基元分支是 `let _ = encode_prim(..)` ⇒ helper 返回 0
/// （「成功」）而字段一字未动 —— 用户拿到的是静默失败。
#[test]
fn null_into_primitive_field_raises_instead_of_silently_doing_nothing() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    let h = holder();
    frame.regs[0] = h.clone();

    // 先写一个正常值，确认字段可写（否则下面的「旧值不变」断言是空的）。
    frame.regs[1] = Value::I64(7);
    assert_eq!(set(&mut frame, &ctx, "n"), 0, "正常整数必须写得进");
    assert!(matches!(read_n(&h), Value::I64(7)), "写入后应读回 7");

    // 被测：Null 进基元槽。
    frame.regs[1] = Value::Null;
    let rc = set(&mut frame, &ctx, "n");

    assert_eq!(rc, 1, "Null 写进基元字段必须报异常（rc=0 即回归成静默失败）");
    let exc = take_exception(&vm);
    assert!(exc.is_some(), "rc=1 必须伴随真的置了异常，否则调用方无从得知");
    assert!(matches!(read_n(&h), Value::I64(7)), "报错之后字段必须保持旧值 7");
}

/// 对照 ①：引用字段（`string`）写 `Null` **完全合法** —— 走 ref 槽分支，
/// 在 `encode_prim` 之前就 return。这条不钉，「把整条路堵死」也会让上面那条变绿。
#[test]
fn null_into_reference_field_stays_legal() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    frame.regs[0] = holder();

    frame.regs[1] = Value::Str("hi".into());
    assert_eq!(set(&mut frame, &ctx, "s"), 0, "字符串写得进");

    frame.regs[1] = Value::Null;
    assert_eq!(set(&mut frame, &ctx, "s"), 0, "引用字段写 null 是合法的，不得报异常");
    assert!(take_exception(&vm).is_none(), "不得置异常");
}

/// 对照 ②：非 Null 的**类型不符**值（字符串进 `int` 槽）也该报 —— 说明这道门判的是
/// 「这个值能不能编码进这个槽」，而不是特判 Null 一种形态。
#[test]
fn type_mismatch_into_primitive_field_also_raises() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    frame.regs[0] = holder();

    frame.regs[1] = Value::Str("not a number".into());
    assert_eq!(set(&mut frame, &ctx, "n"), 1, "字符串写进 int 槽必须报异常");
    assert!(take_exception(&vm).is_some());
}
