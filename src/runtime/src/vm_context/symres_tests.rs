//! `call_arity` —— 调用目标签名与实参数的精确匹配（fix-call-arity-skew，站点 ⑤ 推广到普通调用）。
//!
//! 判据：物理实参数 == `param_count`（含 `this`）+ sret 隐藏返回槽；`params` 变长无上界。
//! 两条都来自全量 `xtask test` 上的探针普查，不是推断：
//!   · 合法调用里「实参数 < 形参数」**0 次** —— 默认值由调用点在编译期填满（跨包构造器那一支由 #623 补齐）。
//!     故下界是 `param_count`，**不再读 `min_arg`**（此前那段「两种口径并存必须夹住」的补丁随之作废）。
//!   · 「实参数 = 形参数 + 1」有 10 个合法站点，全是返回 blob 值 struct 的函数（sret）。运行时靠
//!     `METHOD_FLAG_SRET`（zbc 1.40）精确识别，而不是容一或猜。

use super::symres::call_arity;
use crate::metadata::bytecode::{BasicBlock, Function, Terminator, METHOD_FLAG_SRET, METHOD_FLAG_VIRTUAL};
use crate::metadata::types::ExecMode;

/// 造一个只有 arity 相关字段有意义的 `Function`（`call_arity` 只读 param_count / method_flags / params_from）。
fn f(param_count: usize, min_arg: u16, params_from: u8, method_flags: u8) -> Function {
    Function {
        name: "T.M".to_string(),
        param_count,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: Vec::new(),
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: false,
        visibility: 0,
        method_flags,
        min_arg,
        params_from,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
        owner_init: Default::default(),
        id: Default::default(),
        forward: Default::default(),
    }
}

#[test]
fn exact_arity_accepts_only_its_own_count() {
    // `x.Label(string)`：物理 2 槽（this + prefix）。skew 下调用点按 v2 `Label()` 只传 this。
    let a = call_arity(&f(2, 1, 0xFF, 0));
    assert!(!a.accepts(1), "少一个实参 = 被调方新加了参数，必须判不接受（实测 `label null7` 的那条）");
    assert!(a.accepts(2));
    assert!(!a.accepts(3), "多一个实参 = 被调方删了参数，也不接受");
}

#[test]
fn optional_tail_does_not_widen_the_lower_bound() {
    // `T(int w, int h = 3)`：物理 3 槽。默认值由调用点填 ⇒ 合法调用恒传 3。
    // 🔴 旧判据用 `min_arg` 当下界、放行 2 —— 那恰恰漏掉「被调方加了一个可选参数」的 skew。
    let a = call_arity(&f(3, 1, 0xFF, 0));
    assert!(!a.accepts(2), "下界不读 min_arg");
    assert!(a.accepts(3));
}

#[test]
fn sret_adds_exactly_one_hidden_slot() {
    // `Point GetPt()`（实例、返回 blob struct）：param_count 1（this）+ sret ⇒ 物理 2。普查实例：`Box.GetPt phys=2 pc=1`。
    let a = call_arity(&f(1, 0, 0xFF, METHOD_FLAG_SRET));
    assert_eq!((a.min, a.max), (2, 2));
    assert!(!a.accepts(1), "漏传 sret 槽不接受");
    assert!(!a.accepts(3));
}

#[test]
fn without_the_sret_bit_the_extra_slot_is_a_mismatch() {
    // 同样的形参数、没有 sret 位 ⇒ +1 就是 skew（「被调方恰好少了一个参数」—— 容一方案永远抓不到的那种）。
    let a = call_arity(&f(1, 0, 0xFF, 0));
    assert!(!a.accepts(2));
}

#[test]
fn sret_bit_composes_with_other_method_flags() {
    // 位是按位或进去的：virtual + sret 仍按 sret 计。
    let a = call_arity(&f(2, 1, 0xFF, METHOD_FLAG_VIRTUAL | METHOD_FLAG_SRET));
    assert_eq!((a.min, a.max), (3, 3));
}

#[test]
fn params_tail_has_no_upper_bound() {
    // `M(params int[] xs)`：调用点通常已打包成一个数组，但未打包的形状也不能误判。
    let a = call_arity(&f(2, 1, 1, 0));
    assert!(!a.accepts(1));
    assert!(a.accepts(2));
    assert!(a.accepts(9), "params 变长 ⇒ 无上界");
}

#[test]
fn min_arg_is_ignored_entirely() {
    // 没被 `_fillParamMeta` 覆盖的合成函数，`MinArg` 停在物理总数；旧实现为此专门夹取。
    // 新判据根本不读它 —— 不论 min_arg 写成什么，结论只看 param_count / sret / params。
    for m in [0u16, 1, 2, 7, u16::MAX] {
        let a = call_arity(&f(2, m, 0xFF, 0));
        assert_eq!((a.min, a.max), (2, 2), "min_arg={m}");
    }
}

// ── 站点 ③ 的判据（encode-ctorless-objnew）──────────────────────────────────
//
// 钉的是「正向位缺席即保守」这条不变式：`ctor_known == false` 且零实参时必须**放行**，
// 因为那正是 zbc 1.39 之前全部产物的形态，也是「这个类本来就没有构造器」的合法形态。
// 判错的代价是静默跳过一个真实存在的构造器——比它要修的 bug 更坏。

use super::symres::ctor_missing_is_definite;

#[test]
fn bare_alloc_empty_ctor_name_is_never_missing() {
    // `IrLoopAllocReuse._bareObjNew` 的裸分配：空名没有指名任何构造器。
    assert!(!ctor_missing_is_definite("", 0, false));
    assert!(!ctor_missing_is_definite("", 0, true));
    assert!(!ctor_missing_is_definite("", 3, true));
}

#[test]
fn unmarked_zero_arg_site_is_let_through() {
    // 保守态：编译期没证出来 ⇒ 与 zbc 1.39 之前逐字一致（零初始化，不抛）。
    assert!(!ctor_missing_is_definite("Ns.C.C", 0, false));
}

#[test]
fn marked_zero_arg_site_is_definite() {
    // 正向位就是本 change 关掉的那个缺口：编译期确实看见过这个构造器 ⇒ 解析不到即缺失。
    assert!(ctor_missing_is_definite("Ns.C.C", 0, true));
}

#[test]
fn args_alone_still_decide_without_the_marker() {
    // 并集下限：没有构造器的类不可能接受实参，与正向位无关（站点 ③ 的原判据，不得退化）。
    assert!(ctor_missing_is_definite("Ns.C.C$1", 1, false));
    assert!(ctor_missing_is_definite("Ns.C.C$1", 1, true));
}

// ── 读到 `Null` 的值类型静态字段：按声明类型补零值（`static_zero_value`）──────────────────
//
// 钉的是「静态存储的默认初始化归 VM，且认得**所有**值类型」：基元、枚举、值 struct（零值 = 一个
// 全零的盒子，编译器把 struct 静态字段存成盒子）。回归来源：只认基元标签时 `static Point P;`
// 一读就崩在 `StructCopy src: … got Null`，`static Color C;` 读出 `null`。

use super::symres::{static_zero_value, verify_static_field, StaticNullVerdict};
use crate::metadata::bytecode::{FieldDesc, Module, CLASS_FLAG_ENUM, CLASS_FLAG_STRUCT};
use crate::metadata::name_index::NameIndex;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{StructFieldLayout, StructTypeLayout, TypeDesc, TypeDescCold};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::sync::Arc;

fn type_desc(name: &str, class_flags: u8, cold: TypeDescCold) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        name: name.into(), class_flags, fields: Vec::new(), field_index: NameIndex::new(),
        vtable: Vec::new(), vtable_index: NameIndex::new(), base_name: None, visibility: 0,
        cold: Some(Box::new(cold)), id: TypeId::UNRESOLVED,
    })
}

/// `{ int a; int b; }`：两条 i32 叶子 @0/@4。
fn two_int_struct(name: &str) -> Arc<TypeDesc> {
    let layout = StructTypeLayout {
        size: 8, ref_offsets: Box::new([]), ref_kinds: Box::new([]),
        fields: Box::new([
            StructFieldLayout { offset: 0, size: 4, kind: 0 },
            StructFieldLayout { offset: 4, size: 4, kind: 0 },
        ]),
    };
    type_desc(name, CLASS_FLAG_STRUCT, TypeDescCold { struct_layout: Some(Arc::new(layout)), ..Default::default() })
}

fn static_field(name: &str, tag: &str) -> FieldDesc {
    FieldDesc { name: name.into(), type_tag: tag.into(), attributes: Box::new([]), visibility: 0 }
}

/// 装一个惰性加载器（无依赖包目录）并把 `types` 入表：`Demo.Point` / `Demo.Pair<int,long>`（值 struct）、
/// `Demo.Pair`（泛型 struct 定义）、`Demo.Color`（枚举）、`Demo.Box`（类）、两个持有静态字段的属主。
fn vm_with_types() -> std::pin::Pin<Box<VmContext>> {
    let vm = VmContext::new();
    vm.install_lazy_loader(None, 0);
    let types = [
        two_int_struct("Demo.Point"),
        two_int_struct("Demo.Pair"),
        two_int_struct("Demo.Pair<int,long>"),
        type_desc("Demo.Color", CLASS_FLAG_ENUM, TypeDescCold::default()),
        type_desc("Demo.Box", 0, TypeDescCold::default()),
        type_desc("Demo.S", 0, TypeDescCold {
            static_fields: Box::new([static_field("P", "Demo.Point"), static_field("B", "Demo.Box")]),
            ..Default::default()
        }),
        // 擦除泛型定义 `class G<T> { static T V; }`：tag 是未代换的 `T`。
        type_desc("Demo.G", 0, TypeDescCold {
            type_params: Box::new(["T".to_string()]),
            static_fields: Box::new([static_field("V", "T")]),
            ..Default::default()
        }),
    ];
    if let Some(l) = vm.core.lazy_loader.write().as_mut() {
        for td in types { l.insert_type(td.name.clone(), td); }
    }
    vm
}

fn empty_module() -> Module {
    Module {
        name: "test".to_owned(), string_pool: vec![], classes: vec![], functions: vec![],
        type_registry: rustc_hash::FxHashMap::default(), func_index: rustc_hash::FxHashMap::default(),
    }
}

/// `v` 是 `type_name` 的装箱 struct，且整块为零。
fn assert_zero_box(v: Value, type_name: &str) {
    let Value::BoxedStruct(b) = v else { panic!("expected a boxed struct, got {v:?}") };
    let o = b.borrow();
    assert_eq!(o.type_desc.name, type_name);
    let (bytes, refs) = o.boxed_blob();
    assert_eq!(bytes.len(), 8);
    assert!(bytes.iter().all(|&x| x == 0), "zeroed payload: {bytes:?}");
    assert!(refs.is_empty());
}

fn unresolved_was_recorded(vm: &VmContext, name: &str) -> bool {
    vm.core.lazy_loader.read().as_ref().is_some_and(|l| l.known_unresolved_type_ro(name))
}

#[test]
fn zero_of_every_value_type_kind() {
    let vm = vm_with_types();
    let _heap = crate::gc::ambient::HeapGuard::enter(vm.heap());
    assert_eq!(static_zero_value(&vm, "int"), Value::I64(0));
    assert_eq!(static_zero_value(&vm, "double"), Value::F64(0.0));
    assert_eq!(static_zero_value(&vm, "Demo.Color"), Value::I64(0), "enums are i64-backed");
    assert_zero_box(static_zero_value(&vm, "Demo.Point"), "Demo.Point");
    // 每次都是新盒子（装入哪一个由 `StaticTable::init_default` 决定）。
    match (static_zero_value(&vm, "Demo.Point"), static_zero_value(&vm, "Demo.Point")) {
        (Value::BoxedStruct(a), Value::BoxedStruct(b)) => assert!(!crate::gc::ObjRef::ptr_eq(&a, &b)),
        other => panic!("{other:?}"),
    }
}

/// 字段类型拼写是源码口径（实参间带空格），盒子必须是**身份名**那个类型 —— 读方 `AsCast` 按它拆箱。
#[test]
fn generic_struct_instantiation_resolves_by_its_identity_name() {
    let vm = vm_with_types();
    let _heap = crate::gc::ambient::HeapGuard::enter(vm.heap());
    assert_zero_box(static_zero_value(&vm, "Demo.Pair<int, long>"), "Demo.Pair<int,long>");
}

#[test]
fn reference_types_read_null_without_probing_unresolvable_names() {
    let vm = vm_with_types();
    let _heap = crate::gc::ambient::HeapGuard::enter(vm.heap());
    for tag in ["string", "object", "Demo.Box", "Demo.Point[]", "unknown", "Demo.Box<int>"] {
        assert_eq!(static_zero_value(&vm, tag), Value::Null, "{tag}");
    }
    // 引用型的泛型实例化只查定义（类 ⇒ 到此为止），从不按实例化名去查：查不到的名字会让加载器
    // 走 Fallback B（加载全部包）。数组、`unknown` 哨兵同理一律不查。
    for name in ["Demo.Box<int>", "Demo.Point[]", "unknown"] {
        assert!(!unresolved_was_recorded(&vm, name), "{name} must not be looked up");
    }
}

#[test]
fn verify_fills_struct_statics_and_leaves_reference_and_open_type_param_statics_null() {
    let vm = vm_with_types();
    let _heap = crate::gc::ambient::HeapGuard::enter(vm.heap());
    let m = empty_module();
    match verify_static_field(&vm, &m, "Demo.S.P") {
        StaticNullVerdict::Default(v) => assert_zero_box(v, "Demo.Point"),
        _ => panic!("a value-struct static reads its zero"),
    }
    assert!(matches!(verify_static_field(&vm, &m, "Demo.S.B"), StaticNullVerdict::Ok), "class static: null is legal");
    // 未代换的型参：实参不知道 ⇒ 保持 Null，且绝不把 `T` 当类型名去查。
    assert!(matches!(verify_static_field(&vm, &m, "Demo.G.V"), StaticNullVerdict::Ok));
    assert!(!unresolved_was_recorded(&vm, "T"));
    assert!(matches!(verify_static_field(&vm, &m, "Demo.S.Nope"), StaticNullVerdict::Missing(_)));
}
