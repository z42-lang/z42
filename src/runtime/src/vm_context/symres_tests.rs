//! `ctor_arity` 的口径夹取（fix-ctor-arity-skew 站点 ⑤）。
//!
//! 这几条钉的是 `min_arg` 那个**两种口径并存**的坑：文档口径是逻辑必填数（不含 `this`），
//! 但 `IrFunction` 的构造器默认值写的是**物理**总数（含 `this`）。夹取一旦去掉，没被
//! `_fillParamMeta` 覆盖过的合成函数会被判成「实参不够」——把合法构造判成版本 skew。

use super::symres::ctor_arity;
use crate::metadata::bytecode::{BasicBlock, Function, Terminator};
use crate::metadata::types::ExecMode;

/// 造一个只有 arity 相关字段有意义的 `Function`（`ctor_arity` 只读这三个）。
fn f(param_count: usize, min_arg: u16, params_from: u8) -> Function {
    Function {
        name: "T.T".to_string(),
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
        method_flags: 0,
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
fn all_required_instance_ctor_accepts_exactly_its_arity() {
    // `T(int v)`：物理 2 槽（this + v），逻辑必填 1。
    let a = ctor_arity(&f(2, 1, 0xFF));
    assert!(!a.accepts(1), "只传 this ⇒ 少一个实参，必须判不接受");
    assert!(a.accepts(2));
    assert!(!a.accepts(3), "多给实参也不接受");
}

#[test]
fn optional_tail_widens_the_lower_bound_only() {
    // `T(int w, int h = 3)`：物理 3 槽，逻辑必填 1 ⇒ 物理可接受 2..=3。
    let a = ctor_arity(&f(3, 1, 0xFF));
    assert!(!a.accepts(1));
    assert!(a.accepts(2), "省略带默认值的尾参（跨包路径不在调用点填）必须放行");
    assert!(a.accepts(3));
    assert!(!a.accepts(4));
}

#[test]
fn params_tail_has_no_upper_bound() {
    // `T(params int[] xs)`：调用点通常已打包成一个数组，但未打包的形状也不能误判。
    let a = ctor_arity(&f(2, 1, 0));
    assert!(a.accepts(2));
    assert!(a.accepts(9), "params 变长 ⇒ 无上界");
}

#[test]
fn physical_min_arg_default_is_clamped_not_trusted() {
    // 🔴 核心回归：没被 `_fillParamMeta` 覆盖的合成函数，`MinArg` 停在**物理**总数
    // （`IrFunction` 构造器里的 `this.MinArg = paramCount`）。不夹取就会算出下界 3、
    // 把唯一合法的 2 判成「少一个实参」——合法构造被当成版本 skew 抛异常。
    let a = ctor_arity(&f(2, 2, 0xFF));
    assert_eq!((a.min, a.max), (2, 2));
    assert!(a.accepts(2), "夹取后默认情形退化成「全必填」——正是该默认值本来的语义");
}

#[test]
fn zero_param_static_like_entry_still_bounded() {
    // 形参数 0（不该出现在 ctor 键上，但解析到这种东西本身就是 skew）：传 this 即越界。
    let a = ctor_arity(&f(0, 0, 0xFF));
    assert!(!a.accepts(1));
}
