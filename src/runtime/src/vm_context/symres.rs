//! 符号解析失败的「确定不存在」判定（fix-silent-symbol-resolution）。
//!
//! # 为什么需要一个专门的判定
//!
//! VM 的 `UNRESOLVED` 哨兵**同时编码「跨包待解析」和「根本不存在」**，下游兜底一律按
//! 前者处理——于是「依赖版本 skew」全部退化成静默的错误答案。要把它改成报错，就必须
//! 先能区分这两件事：**所有惰性加载路径都穷尽之后仍解析不到**，才叫「确定不存在」。
//!
//! # 为什么不能靠值判断
//!
//! 静态字段读不到时返回 `Value::Null`，而 `Null` **本身是合法值**（未赋值的引用型静态
//! 字段就是 Null）。所以不能用「读到 Null 就报错」——那会把正常程序打崩。判据只能是
//! **元数据**：该字段是否真的声明在属主类型（或其基类）上。
//!
//! # 热路径代价
//!
//! 校验只在**读到 Null 时**才做（非 Null 读零成本）。Null 读本就少见，且真正走到查表的
//! 只有那一小撮。宁可多查几次，也不要为了省这点开销去用「值是不是 Null」这种不可靠判据。
//!
//! # 保守原则
//!
//! **只在确信时才报错。** 属主类型解析不出来时**不报**（那是另一类缺失，归 ObjNew 那个
//! 站点管），保持既有行为。宁可漏掉一些，也不要把正常的跨包加载变成崩溃。

use crate::metadata::{Module, TypeDesc};
use crate::vm_context::VmContext;

/// 在该类型或其基类链上找静态字段的声明，返回其 `type_tag`。
///
/// `Err(())` = 基类链中途解析不出来 ⇒ **无法证明「没声明」**，调用方须保守放行。
fn lookup_static_field_tag(
    ctx: &VmContext, module: &Module, td: &TypeDesc, field: &str,
) -> Result<Option<String>, ()> {
    let mut cur: Option<std::sync::Arc<TypeDesc>> = None;
    let mut probe: &TypeDesc = td;
    // 基类链深度有限；每层只做一次线性扫（静态字段个数通常个位数）。
    for _ in 0..64 {
        if let Some(f) = probe.static_fields().iter().find(|f| f.name == field) {
            return Ok(Some(f.type_tag.clone()));
        }
        let Some(base) = probe.base_name.as_deref() else { return Ok(None) };
        let next = module.type_registry.get(base).cloned()
            .or_else(|| ctx.try_lookup_type(base));
        match next {
            None => return Err(()),   // 基类不可解析 → 不下结论
            Some(b) => { cur = Some(b); probe = cur.as_deref().unwrap(); }
        }
    }
    Err(())
}

/// 读静态字段得到 `Null` 之后的裁决。
pub enum StaticNullVerdict {
    /// 合法的 `Null`（引用型字段未赋值，或无法证明有问题）——照常返回。
    Ok,
    /// 字段已声明且是**值类型**，槽位却是 `Null` ⇒ 该槽从未被零初始化。
    /// 用声明类型的零值补上（调用方应**回写槽位**，相当于惰性零初始化）。
    Default(crate::metadata::Value),
    /// 确定不存在 ⇒ 抛该异常值。
    Missing(crate::metadata::Value),
}

/// 读静态字段得到 `Null` 后的裁决。见 [`StaticNullVerdict`]。
pub fn verify_static_field(
    ctx: &VmContext, module: &Module, field_fq: &str,
) -> StaticNullVerdict {
    let Some((owner, name)) = field_fq.rsplit_once('.') else {
        return StaticNullVerdict::Ok;   // 无点 = 全局静态字段，不归任何类型
    };
    let resolved = module.type_registry.get(owner).cloned()
        .or_else(|| ctx.try_lookup_type(owner));
    let Some(td) = resolved else {
        // 属主类型整个解析不出来。`try_lookup_type` **本身就是完整解析路径**（会触发所属包
        // 加载），它失败就再无回落 ⇒ 确定不存在。
        //
        // 这条最初被我当成「归 ObjNew 那个站点管」而保守跳过，结果本站点对**最常见的
        // skew 形态**（整个依赖包不在了）完全不生效——实测才发现。保守要有边界：
        // 保守的是「解析路径没走完就别下结论」，不是「凡是拿不到就别报」。
        return StaticNullVerdict::Missing(crate::exception::make_missing_symbol_exception(
            ctx, module,
            format!("static field `{field_fq}`: type `{owner}` could not be resolved"),
        ));
    };
    match lookup_static_field_tag(ctx, module, &td, name) {
        Err(()) => StaticNullVerdict::Ok,        // 基类链没走通 → 不下结论
        Some_tag @ Ok(Some(_)) => {
            let tag = match Some_tag { Ok(Some(t)) => t, _ => unreachable!() };
            // fix-static-value-field-null-slot：值类型静态字段无初始化器时槽位停在 `Null`
            // （`resize_with(|| Value::Null)` 只填 Null，没人按声明类型零初始化）⇒
            // `static int N;` 一读就崩在 `__box_prim: expected integer value, got Null`。
            // 这里按声明类型补零值；调用方回写槽位，后续读不再走这条路。
            let d = crate::metadata::types::default_value_for(&tag);
            if matches!(d, crate::metadata::Value::Null) {
                StaticNullVerdict::Ok            // 引用型的零值就是 Null —— 合法
            } else {
                StaticNullVerdict::Default(d)
            }
        }
        Ok(None) => StaticNullVerdict::Missing(crate::exception::make_missing_symbol_exception(
            ctx, module,
            format!("static field `{field_fq}` is not declared on type `{owner}`"),
        )),
    }
}
