//! fix-missing-write-barriers (2026-10-07)：VM 自己（不经 `FieldSet` / `ArraySet` 指令）往**已存在**
//! 的堆对象里写引用的站点，必须和指令一样发写屏障。
//!
//! 每个用例同一个形状，也就是这类 bug 在真实程序里出事的形状：
//!
//! 1. 目标对象经真实的 minor **晋升成老年代**：挂在一个 pinned holder 的字段上（holder 是根，目标不是——
//!    minor 对老根只「穿过」一层、不把老孩子入队，所以目标的孩子此后**只能经卡表**被找到）；
//! 2. 经被测站点往它里面写一个**新分配（年轻）**的引用；
//! 3. 卡表不变量当场核对（`verify_card_invariant`：老对象持有年轻引用 ⇒ 所在卡必脏）；
//! 4. 再跑一次 minor，年轻对象必须还活着（对象用弱引用判，字符串用块头的 `is_live`）。
//!
//! 修复前，第 3 步报出那个老对象、第 4 步年轻对象被扫掉（use-after-free 的前一步）。

use crate::gc::region::PROMOTION_THRESHOLD;
use crate::gc::types::RootHandle;
use crate::gc::{GcMode, GcRef};
use crate::metadata::bytecode::{Module, CLASS_FLAG_STRUCT};
use crate::metadata::types::{
    ArrayObj, ElemType, FieldSlot, NativeData, StructTypeLayout, TypeDesc, TypeDescCold,
    STRUCT_REF_GCREF, TAG_OBJECT,
};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::pin::Pin;
use std::sync::Arc;

fn gen_ctx() -> Pin<Box<VmContext>> {
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
    ctx
}

/// 引用字段齐全的普通类：`fields` 依次是槽 0, 1, …（类型 `object` ⇒ 都是引用槽）。
fn class_td(name: &str, fields: &[(&str, &str)]) -> Arc<TypeDesc> {
    let fields: Vec<FieldSlot> = fields.iter().map(|(n, t)| FieldSlot {
        name: (*n).into(), type_tag: (*t).into(), visibility: 0,
    }).collect();
    let field_index: crate::metadata::NameIndex =
        fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.to_string(), base_name: None,
        fields, field_index, vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None, id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

fn age(v: &Value) -> u8 {
    match v {
        Value::Object(gc) | Value::BoxedStruct(gc) => GcRef::gen_age(gc),
        Value::Array(gc) => GcRef::gen_age(gc),
        other => panic!("not a carded heap value: {other:?}"),
    }
}

/// 把 `values` 经真实的 minor 晋升成老年代：挂在一个 pinned holder 的字段上存活够次数。返回的 pin
/// 让它们此后一直活着（holder 是根，它们不是）。最后多跑一次 minor 把晋升时染的卡洗掉，好让被测写入
/// 从干净的卡开始。
fn promote(ctx: &VmContext, values: &[Value]) -> RootHandle {
    let slots: Vec<(String, String)> =
        (0..values.len()).map(|i| (format!("f{i}"), "object".to_string())).collect();
    let slots: Vec<(&str, &str)> = slots.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    let holder = ctx.heap().alloc_object(class_td("Holder", &slots), values.to_vec(), NativeData::None);
    let pin = ctx.heap().pin_root(holder);
    for _ in 0..=PROMOTION_THRESHOLD {
        ctx.heap().force_collect();
    }
    for v in values {
        assert!(age(v) >= PROMOTION_THRESHOLD, "test setup: {v:?} must be old (age {})", age(v));
    }
    ctx.heap().verify_card_invariant().expect("test setup: the card invariant holds before the store");
    pin
}

fn assert_cards_hold(ctx: &VmContext, site: &str) {
    if let Err(e) = ctx.heap().verify_card_invariant() {
        panic!("{site} stored a young reference into an old object without the write barrier:\n{e}");
    }
}

// ── Exception.StackTrace（throw 点补写）────────────────────────────────────────

fn exception_module() -> Module {
    let td = class_td("Std.Exception", &[
        ("Message", "str"), ("StackTrace", "str"), ("InnerException", "Std.Exception"),
    ]);
    let mut type_registry = rustc_hash::FxHashMap::default();
    type_registry.insert("Std.Exception".to_string(), td);
    Module {
        name: "test".into(), string_pool: vec![], classes: vec![], functions: vec![],
        type_registry, func_index: rustc_hash::FxHashMap::default(),
    }
}

/// 异常对象通常早就老了（构造、存起来、晚些才抛），throw 点却当场分配 trace 字符串：一次老→年轻写入。
/// 修复前 `populate_stack_trace` 直接 `set_field_value`，卡不脏，下一次 minor 扫掉 trace。
#[test]
fn stack_trace_written_into_an_old_exception_survives_the_next_minor() {
    let module = exception_module();
    let ctx = gen_ctx();
    // trace 字符串经 ambient heap 分配——没有它就是脱离 GC 的泄漏块，测不出任何东西。
    let _heap = crate::gc::ambient::HeapGuard::enter(ctx.heap());
    let exc = crate::exception::make_stdlib_exception(&ctx, &module, "Std.Exception", "boom".into())
        .expect("constructs");
    let _pin = promote(&ctx, std::slice::from_ref(&exc));

    crate::exception::populate_stack_trace(&exc, &ctx, &module);

    let Value::Object(gc) = &exc else { panic!("exception is an object") };
    let slot = gc.type_desc().field_index.get("StackTrace").copied().expect("StackTrace slot");
    let Value::Str(trace) = gc.borrow().field_value(slot) else { panic!("StackTrace populated") };
    assert!(trace.gen_age() < PROMOTION_THRESHOLD, "test setup: the trace string is young");
    assert_cards_hold(&ctx, "populate_stack_trace");

    ctx.heap().force_collect();
    assert!(trace.var_ref().is_live(),
        "the trace string was swept while the old exception still referenced it");
}

// ── 反射 FieldInfo.SetValue ──────────────────────────────────────────────────

/// 反射写字段（反序列化把成员绑到普通字段上的常态）是用户代码可达的老→年轻写入。
/// 修复前 `__field_set_value` 直接 `try_set_field_value`：SATB 那一半照记，卡表那一半没发。
#[test]
fn field_info_set_value_into_an_old_object_survives_the_next_minor() {
    let ctx = gen_ctx();
    let target = ctx.heap().alloc_object(class_td("Demo.Target", &[("Child", "object")]), vec![], NativeData::None);
    let _pin = promote(&ctx, std::slice::from_ref(&target));
    // `FieldInfo` 的替身：`__field_set_value` 只读它的 `Name`。
    let field_info = ctx.heap().alloc_object(
        class_td("Std.Reflection.FieldInfo", &[("Name", "str")]),
        vec![Value::Str("Child".into())], NativeData::None);
    let young = ctx.heap().alloc_object(class_td("Demo.Young", &[]), vec![], NativeData::None);
    let weak = ctx.heap().make_weak(&young).expect("objects are weakly referenceable");

    crate::corelib::reflection::accessors::builtin_field_set_value(
        &ctx, &[field_info, target.clone(), young]).expect("SetValue ok");

    assert_cards_hold(&ctx, "FieldInfo.SetValue");
    ctx.heap().force_collect();
    assert!(ctx.heap().upgrade_weak(&weak).is_some(),
        "the value stored by FieldInfo.SetValue was swept while the old target still referenced it");
}

// ── 装箱 struct 写进 struct[]（Array.SetValue / ArraySet）─────────────────────

/// `struct S { object R; }`——一个引用叶子。
fn struct_s() -> (Arc<TypeDesc>, Arc<StructTypeLayout>) {
    let layout = Arc::new(StructTypeLayout {
        size: 8, ref_offsets: Box::new([0]), ref_kinds: Box::new([STRUCT_REF_GCREF]), fields: Box::new([]),
    });
    let td = Arc::new(TypeDesc {
        class_flags: CLASS_FLAG_STRUCT, visibility: 0, name: "Demo.S".to_string(), base_name: None,
        fields: Vec::new(), field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { struct_layout: Some(layout.clone()), ..Default::default() })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    (td, layout)
}

/// `set_boxed` 把盒子的引用叶子**拷进** `struct[]`，数组持有的是叶子、不是盒子。修复前屏障拿**盒子**
/// 判龄：盒子老、叶子年轻 ⇒ 数组的卡不染。之后盒子的叶子一改（卡随之被 minor 洗掉），叶子就只剩数组
/// 这一条路，下一次 minor 把它扫掉。
#[test]
fn a_boxed_struct_copied_into_an_old_struct_array_keeps_its_young_leaf_alive() {
    let ctx = gen_ctx();
    let (s_td, layout) = struct_s();
    let arr = ctx.heap().alloc_array_obj(
        ArrayObj::struct_backed(ctx.heap(), ElemType::intern("Demo.S"), 1, layout));
    let Value::Object(box_gc) = ctx.heap().alloc_object(s_td, vec![], NativeData::None) else {
        panic!("box allocates")
    };
    let boxed = Value::BoxedStruct(box_gc);
    let _pin = promote(&ctx, &[arr.clone(), boxed.clone()]);

    // 老盒子收到一个年轻叶子——走正常的（带屏障的）叶子写入，盒子的卡因此是脏的。
    let young = ctx.heap().alloc_object(class_td("Demo.Young", &[]), vec![], NativeData::None);
    let weak = ctx.heap().make_weak(&young).expect("objects are weakly referenceable");
    super::struct_leaf::struct_field_set_val(&ctx, &boxed, 0, TAG_OBJECT, &young).expect("leaf write");

    // `arr.SetValue(box, 0)`：叶子被拷进老数组。
    crate::corelib::array::builtin_array_set(&ctx, &[arr.clone(), boxed.clone(), Value::I64(0)])
        .expect("SetValue ok");
    assert_cards_hold(&ctx, "Array.SetValue(boxed struct → struct[])");

    // 盒子不再持有它 ⇒ 年轻对象只剩数组元素这一条路。
    super::struct_leaf::struct_field_set_val(&ctx, &boxed, 0, TAG_OBJECT, &Value::Null).expect("leaf clear");
    ctx.heap().force_collect();
    assert!(ctx.heap().upgrade_weak(&weak).is_some(),
        "the struct[] element's reference leaf was swept while the old array still held it");
}
