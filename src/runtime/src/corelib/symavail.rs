//! `__sym_available` —— `available!(X)` 的运行期回落（add-symbol-availability-macro）。
//!
//! **正常路径永不执行。** 这条 builtin 在模块加载期就被
//! [`crate::metadata::loader::fold_availability`] 折成 `ConstBool` 并连带剪掉死分支。
//!
//! 那为什么还要它？因为「折叠 pass 没跑」必须**可被发现**。如果这里静默返回一个看似
//! 合理的值，pass 失效就会退化成「每次调用查一次表」——结果照样对、只是慢，且死分支
//! 根本没被剪掉（`available!` 的**全部意义**就在于剪掉它）。那就成了又一个从不报警的
//! 假保障。故：debug 下直接 panic 点名，release 下告警 + 保守答 false。
//!
//! 保守答 false 而不是 true：false 让代码走 fallback 分支（总是安全的老路径），
//! true 会让它走进一条它以为存在、实则可能不存在的分支。

use super::*;

pub fn builtin_sym_available(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let key = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => "<non-string>".to_string(),
    };

    debug_assert!(
        false,
        "__sym_available(`{key}`) reached at runtime — the load-time \
         `fold_availability` pass did not run. Dead branches were NOT pruned; \
         see metadata/loader/availability.rs."
    );

    tracing::warn!(
        "__sym_available(`{key}`) reached at runtime — fold_availability did not run; \
         dead branches were not pruned. Answering `false` (fallback path)."
    );
    Ok(Value::Bool(false))
}
