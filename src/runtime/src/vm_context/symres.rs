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
        Ok(Some(tag)) => {
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

// ── 站点 ③：ObjNew 的构造器解析不到 ─────────────────────────────────────────

/// `ObjNew` 的 ctor 名在合并模块和惰性加载器里都解析不到时的裁决。
/// `Some(exc)` = 确定不存在，抛之；`None` = 无法证明有问题，照常走「无 ctor」路径
/// （对象已零初始化）。
///
/// # 判据为什么是「有没有实参」
///
/// z42c 对**没有构造器**的类照样发射 `ObjNew`，ctor 键取裸类名（`Demo.Point.Point`）——
/// 而这与**单构造器**的 primary 裸键（`stabilize-instance-dispatch-keys`）**同形**。
/// 也就是说运行时**无法**从名字本身区分「这个类没有构造器」和「构造器应该在但不见了」。
/// 另有 `IrLoopAllocReuse` 的裸分配（ctor 名为空串）也走这条路。
///
/// 唯一可证的事实是：**没有构造器的类不可能接受实参**。所以 `argc > 0` 且全路径解析不到
/// ⇒ 必然是「本该存在的构造器不见了」（依赖包版本 skew 的典型形态），定案报错。
///
/// # 残留缺口（已知、有意保留）
///
/// `argc == 0` 时无法区分「本来就无 ctor」与「`C()` 在旧依赖里不存在」，仍按旧行为默认初始化。
///
/// 原先记的补法是「让 TypeDesc 记录本类声明了哪些构造器」，**经复核几乎没有覆盖面**：
/// `stabilize-instance-dispatch-keys` 规定 primary 构造器用**裸键**，所以「类只要声明了任何
/// 构造器，裸键就一定解析得到」——裸键解析不到时，被加载的那份类几乎必然真的零构造器。
///
/// 真正的闭合要在**调用点**给「零构造器」一个可区分的编码（如 `IrLoopAllocReuse` 早已确立的
/// 空 ctor 名 = 裸分配），于是「非空却解析不到」⇒ 无论 argc 都是缺失。卡点是**判据算不准**：
/// z42c 为「有字段初始化器、无显式 ctor」的类**合成**的隐式构造器（`IrGenTypeEmitter._emitSynthCtor`）
/// 既不是 MethodSymbol、也不在 `_bindNew` 那一刻存在（`_synthCtors` 跑在所有绑定之后），
/// 而准确的 oracle（本包全部已发射函数 ∪ `DependencyIndex.Statics`）要等整包装配后才齐。
/// 判错的代价是**静默跳过真构造器**——比这里要修的 bug 更坏。故另立 change 处理。
///
pub fn missing_ctor_exception(
    ctx: &VmContext, module: &Module, class_name: &str, ctor_name: &str, argc: usize,
) -> Option<crate::metadata::Value> {
    if argc == 0 { return None; }
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "constructor `{ctor_name}` of type `{class_name}` could not be resolved \
             (called with {argc} argument(s)); the loaded package may be older than \
             the one this code was compiled against"
        ),
    ))
}

// ── 站点 ⑤：ObjNew 的构造器解析到了，但签名容不下实参 ───────────────────────

/// 构造器可接受的**物理**实参数区间（含 `this`）。`max == u16::MAX` ⇒ `params` 变长，无上界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CtorArity {
    pub min: u16,
    pub max: u16,
}

impl CtorArity {
    /// 调用方传入 `phys` 个值（含 `this`）时是否可接受。
    #[inline]
    pub fn accepts(&self, phys: usize) -> bool {
        let phys = phys.min(u16::MAX as usize) as u16;
        phys >= self.min && phys <= self.max
    }
}

/// 从函数元数据算出可接受的实参数区间。
///
/// # `min_arg` 的口径不一致，必须夹住
///
/// 文档口径是**逻辑**必填数（不含 `this`，`IrModule.z42` 的 `MinArg` 注释），
/// `IrGenFacts._fillParamMeta` 写的也是逻辑值。但 `IrFunction` 构造器的**默认值**是
/// `MinArg = paramCount`，那是**物理**总数（含 `this`）——`IrGenMemberEmitter` 的注释
/// 明说了这点，并为 getter/setter 手工覆盖。没被 `_fillParamMeta` 覆盖过的合成函数因此
/// 会多算 1，不夹住就会把合法调用判成 skew（假阳性）。
///
/// 夹到 `param_count` 之后，默认情形正好退化成「全必填」——即那个默认值本来的语义。
pub fn ctor_arity(f: &crate::metadata::Function) -> CtorArity {
    let phys = f.param_count.min(u16::MAX as usize) as u16;
    let min = ((f.min_arg as usize).saturating_add(1)).min(f.param_count) as u16;
    // `params` 变长尾参：实参数可以超过形参数（调用点通常已打包，但两种形状都放行）。
    let max = if f.params_from != 0xFF { u16::MAX } else { phys };
    CtorArity { min, max }
}

/// `ObjNew` 解析到构造器**之后**的裁决：形参数容不下实参数 ⇒ 抛。
/// `Some(exc)` = 定案不匹配；`None` = 可接受，照常调用。
///
/// # 为什么解析成功也要查
///
/// 裸键在版本 skew 下会**命中错的构造器**：单构造器必是 primary、必用裸键，于是
/// v2 的 `C()` 与 v1 的 `C(int)` 占用**同一个**裸键。站点 ③（解析不到才抛）在这里
/// 完全不触发——键解析得到，只是解析到了另一个东西。而 `exec_function` 用
/// `Frame::new(args, max_reg)` 建帧，**不做任何 arity 校验**：形参就停在默认值上继续跑，
/// 实测 `new Widget()` 撞上 `Widget(int v)` 把字段静默写成 0。
///
/// 这条比站点 ③ 剩下的 `argc == 0` 缺口常见得多：**任何**「构造器签名变了」的 skew 都落在这里。
///
/// # 为什么复用 `MissingSymbolException`
///
/// 新异常类要先进 stdlib，而冷启动种子的 stdlib 里没有它 ⇒ 得走两-nightly。语义上也说得通：
/// 调用点指名的那个重载**确实不在**，撞上的是同键下的另一个。
pub fn wrong_ctor_arity_exception(
    ctx: &VmContext, module: &Module, class_name: &str, ctor_name: &str,
    arity: CtorArity, argc: usize,
) -> Option<crate::metadata::Value> {
    let phys = argc + 1;   // 实参 + 前置的 this
    if arity.accepts(phys) { return None; }
    let want = if arity.max == u16::MAX {
        format!("at least {}", arity.min.saturating_sub(1))
    } else if arity.min == arity.max {
        format!("{}", arity.min.saturating_sub(1))
    } else {
        format!("{} to {}", arity.min.saturating_sub(1), arity.max.saturating_sub(1))
    };
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "constructor `{ctor_name}` of type `{class_name}` was called with {argc} \
             argument(s) but the loaded one accepts {want}; the loaded package may be \
             older than the one this code was compiled against"
        ),
    ))
}

// ── 站点 ②：ObjNew 的类型解析不到 ───────────────────────────────────────────

/// `ObjNew` 的类型在类型注册表和惰性加载器里都找不到时的裁决。
/// `Some(exc)` = 确定不存在，抛之；`None` = 回落描述符是**合法**的，照旧合成。
///
/// # 判据
///
/// 回落描述符（[`crate::interp::dispatch::make_fallback_type_desc`]）有一个正当用途：
/// 合并进来的 stdlib 模块不带预建 TypeDesc，但带 `ClassDesc`——按 `module.classes` 的
/// 继承链现建一个，字段槽是**齐的**。判据就是这条链在不在：
///
/// - `module.classes` 里有 → 回落描述符正确，放行。
/// - 没有，且名字**不带点** → 编译器合成的本地类（闭包类等），沿用既有行为放行。
/// - 没有，且名字**带点** → 跨包引用没解析到 ⇒ 定案缺失。
///
/// 最后这条此前是一条 `tracing::warn!`：合成出来的空壳没有字段槽，构造器的 `FieldSet`
/// 被**丢弃**、后续 `FieldGet` 全读 Null，错误现场离根因十万八千里（实测：`Std.IO.Process`
/// 被合成空壳后，崩在 `AppendString` 的 `arr.Length`）。日志挡不住这种静默数据损坏——
/// 判据既然已经确定，就该抛。
pub fn missing_type_exception(
    ctx: &VmContext, module: &Module, class_name: &str,
) -> Option<crate::metadata::Value> {
    if !class_name.contains('.') { return None; }
    if module.classes.iter().any(|c| c.name == class_name) { return None; }
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "type `{class_name}` could not be resolved in the module registry, the lazy \
             loader, or this module's class descriptors; the loaded package may be older \
             than the one this code was compiled against"
        ),
    ))
}

// ── 站点 ④：基类解析不到 ─────────────────────────────────────────────────────

/// 类型描述符的继承视图残缺（基类有声明、却合不进来）时的裁决。
/// `Some(exc)` = 基类确定不存在，抛之；`None` = 视图完整，放行。
///
/// # 为什么这条最危险
///
/// 前三个站点丢的是**一个**符号；这里丢的是**整片继承面**——基类的全部字段槽和 vtable
/// 条目一起消失。表现是「子类自己的成员都对、继承来的全是 Null / 派发不到」，而且
/// `FieldSet` 是**静默丢弃**的，错误现场离根因可以隔上任意远。
///
/// # 判据必须在惰性解析走完之后
///
/// 「基类现在不在注册表里」**不等于**不存在：按需加载下它随时可能随下一个 zpkg 到场。
/// 所以调用方必须先走完 `try_lookup_type`（内部会 `ensure_base_chain_loaded` + 把继承
/// fixup 跑到不动点），拿回来的描述符**仍然**带着 `base_unmerged` 旗子，才算定案。
/// 详见 [`crate::metadata::types::TypeDescCold::base_unmerged`]。
pub fn missing_base_exception(
    ctx: &VmContext, module: &Module, td: &TypeDesc,
) -> Option<crate::metadata::Value> {
    if !td.base_unmerged() { return None; }
    let base = td.base_name.as_deref().unwrap_or("<unknown>");
    let name = &td.name;
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "base type `{base}` of `{name}` could not be resolved; every inherited field \
             and virtual method is absent from the layout — the loaded package may be \
             older than the one this code was compiled against"
        ),
    ))
}
