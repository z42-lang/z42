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
