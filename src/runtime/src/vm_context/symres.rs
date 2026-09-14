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
/// # 判据：编译期的正向位 `ctor_known`（zbc 1.39 encode-ctorless-objnew）
///
/// 名字本身分不出两种情况：`stabilize-instance-dispatch-keys` 规定 primary 构造器占**裸键**，
/// 于是 `class C { }`（零构造器）与 `class C { C() {…} }`（单构造器）在调用点发出的
/// ctor 名**同形**（都是 `Ns.C.C`）。另有 `IrLoopAllocReuse` 的裸分配（ctor 名为空串）。
///
/// 解法是让编译器把它**知道的事**写进指令：整包装配完毕后，`CtorKnownFixup` 检查该 ctor 名
/// 是否出现在「本包全部已发射函数 ∪ `DependencyIndex`」里 —— 出现才置 `ctor_known`。于是：
///
/// * `ctor_known == true` + 解析不到 ⇒ **定案缺失**（装的包比编译时旧），抛。
/// * `ctor_known == false` ⇒ 编译期就没看见它（真零构造器 / 裸分配 / 证不出来）⇒ 保守放行。
///
/// **位的缺席是保守态**，这是刻意的：编译期证不出来时行为与 bump 前逐字一致，绝不会因为判错
/// 而静默跳过一个真实存在的构造器 —— 那比这里要修的 bug 更坏。
///
/// # 为什么保留 `argc > 0`
///
/// 两个判据取**并集**，只增不减。`argc > 0` 那条独立成立：没有构造器的类不可能接受实参，
/// 所以带实参却全路径解析不到必然是缺失，与编译器有没有给出正向位无关。
/// 上面那段判据的**纯**形式（无 `VmContext`，可单测穷举）。
///
/// `true` = 解析不到就是定案缺失。三条：
/// * 空名（`IrLoopAllocReuse._bareObjNew` 的裸分配）永远不是缺失 —— 它压根没指名任何构造器。
/// * `ctor_known`（zbc 1.39 正向位）：编译期在「本包已发射函数 ∪ Deps」里看见过它。
/// * `argc > 0`：没有构造器的类不可能接受实参。与正向位取**并集**，只增不减。
pub fn ctor_missing_is_definite(ctor_name: &str, argc: usize, ctor_known: bool) -> bool {
    if ctor_name.is_empty() { return false; }
    ctor_known || argc > 0
}

pub fn missing_ctor_exception(
    ctx: &VmContext, module: &Module, class_name: &str, ctor_name: &str, argc: usize,
    ctor_known: bool,
) -> Option<crate::metadata::Value> {
    if !ctor_missing_is_definite(ctor_name, argc, ctor_known) { return None; }
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "constructor `{ctor_name}` of type `{class_name}` could not be resolved \
             (called with {argc} argument(s)); the loaded package may be older than \
             the one this code was compiled against"
        ),
    ))
}

// ── 站点 ⑤：调用目标解析到了，但签名容不下实参（构造器 + 普通方法调用）──────────

/// 被调函数可接受的**物理**实参数区间：含 `this`（实例方法 / 构造器的 `param_count` 本就含它）、
/// 含 sret 隐藏返回槽。`max == u16::MAX` ⇒ `params` 变长，无上界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallArity {
    pub min: u16,
    pub max: u16,
}

impl CallArity {
    /// 调用方传入 `phys` 个值（含 `this`、含 sret 槽）时是否可接受。
    #[inline]
    pub fn accepts(&self, phys: usize) -> bool {
        let phys = phys.min(u16::MAX as usize) as u16;
        phys >= self.min && phys <= self.max
    }
}

/// 从函数元数据算出可接受的物理实参数区间。
///
/// # 下界是 `param_count`，不是 `min_arg`（fix-call-arity-skew 实测）
///
/// z42 的默认参数由**调用点在编译期填满**（跨包构造器那一支由 #623 补齐）。在全量 `xtask test`
/// 上给解释器的三个函数体入口挂探针普查：合法调用里「实参数 < 形参数」**一次都没有**。所以
/// 任何少于 `param_count` 的调用都是 skew——典型是「被调方新加了一个可选参数」。此前
/// 构造器用 `min_arg` 当下界，恰恰把这种 skew 放过去了；那段为 `min_arg` 两种口径并存而写的
/// 夹取补丁也随之作废。
///
/// # 上界要加 sret，而且必须读元数据
///
/// 同一次普查里有 10 个合法站点「实参数 = 形参数 + 1」，全是返回 blob 值 struct 的函数：
/// caller 在末尾传隐藏返回槽，而它**不计入** `param_count`。运行时本来无从得知——猜
/// （容一 / 复刻编译器的 blob 判定）要么留洞、要么误杀——所以编译器把它写进
/// `METHOD_FLAG_SRET`（zbc 1.40），这里读位，精确。
///
/// `params` 变长尾参：调用点通常已打包成数组（普查中实参数恒等于形参数），但展开形状多出的
/// 实参同样放行 ⇒ 无上界。
pub fn call_arity(f: &crate::metadata::Function) -> CallArity {
    let sret = (f.method_flags & crate::metadata::bytecode::METHOD_FLAG_SRET) != 0;
    let expected = f.param_count.saturating_add(sret as usize).min(u16::MAX as usize) as u16;
    let max = if f.params_from != 0xFF { u16::MAX } else { expected };
    CallArity { min: expected, max }
}

/// 调用解析到目标**之后**的裁决：签名容不下实参数 ⇒ 抛。`Some(exc)` = 定案不匹配；`None` = 可接受。
///
/// # 为什么解析成功也要查
///
/// **primary 裸键**（`stabilize-instance-dispatch-keys`：声明序第一个同名成员用裸名）在版本 skew 下会
/// **命中错的签名**：v2 的 `C()` / `x.Label()` 与 v1 的 `C(int)` / `x.Label(string)` 占用**同一个**键。
/// 「解析不到才抛」在这里完全不触发——键解析得到，只是解析到了另一个东西。而建帧
/// （`Frame::new` / `new_from_regs`）**不做任何 arity 校验**：形参停在默认值上继续跑，实测
/// `new Widget()` 撞 `Widget(int)` 把字段写成 0、`b.Label()` 撞 `Label(string)` 输出 `null7`。
///
/// 覆盖面：构造器、实例方法（`VCall` 与去虚化后的直接 `Call`）、静态虚成员。**常规静态方法天然免疫**——
/// 它们的键恒为全签名 mangle，签名一变键就变、解析失败，由「缺符号」那条路报。
///
/// # 在哪里调
///
/// 只在**首次绑定**处调（resolver 预填 token / 冷路径写回缓存 / PIC 安装 / cross-cell 填充），
/// 命中缓存后不再查 ⇒ 热路径零开销。绑定被拒的站点不会进缓存，所以每次都会重新走到这里抛。
///
/// # 为什么复用 `MissingSymbolException`
///
/// 新异常类要先进 stdlib，而冷启动种子的 stdlib 里没有它 ⇒ 得走两-nightly。语义上也说得通：
/// 调用点指名的那个签名**确实不在**，撞上的是同键下的另一个。
pub fn wrong_arity_exception(
    ctx: &VmContext, module: &Module, callee: &str, arity: CallArity, phys: usize,
) -> Option<crate::metadata::Value> {
    if arity.accepts(phys) { return None; }
    let want = if arity.max == u16::MAX {
        format!("at least {}", arity.min)
    } else {
        format!("{}", arity.min)
    };
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "`{callee}` resolved to a definition whose signature does not match this call \
             (it takes {want} physical argument(s), the call passes {phys}); the loaded package \
             may differ from the one this code was compiled against"
        ),
    ))
}

/// runtime-ambiguous-use-site：**这个名字由两个已加载的 zpkg 各自声明过** ⇒ 用它就是错的。
///
/// 与本模块其余判定同族（都是「派发点的符号完整性」），但根因不同：那些是「装的包比编译时旧」，
/// 这条是「装了两个都提供同一个 FQ 名的包」，谁生效纯看加载顺序。
///
/// # 为什么在**使用位**报而不是加载时报
///
/// 加载是**惰性且按包**的：两个包完全可能在程序从不触碰的名字上冲突（A 因 `A.X` 被加载、
/// B 因 `B.Y` 被加载，而它们碰巧都有 `Ns.W`）。加载时报错 = 为一个程序既没引用、也无权修的
/// 冲突把它打死 —— 这正是编译期 E0601 刻意用「使用位」原则避开的行为，运行期不该反着来。
///
/// # 为什么不是「让解析失败」
///
/// 好几个调用方把 `try_lookup_*` 的 `None` 当**良性**信号：`obj_new` 读作「这个类没有构造
/// 函数」、`vcall_resolve` / `dispatch` 拿它走候选链回退。让歧义名解析失败会把「歧义」变成
/// 「静默跳过构造函数」，比现状更坏。所以解析原样不动，判定挂在真正会派发、且能报错的位置。
///
/// # 热路径代价
///
/// 常态是一次 relaxed 原子读（`ambiguity_seen()`，进程内从没发生过碰撞时恒 false），
/// 只有真出现过碰撞才去拿读锁精确查。
pub fn ambiguous_function_exception(
    ctx: &VmContext, module: &Module, fname: &str,
) -> Option<crate::metadata::Value> {
    if !crate::metadata::lazy_loader::ambiguity_seen() { return None; }
    if !ctx.is_ambiguous_function(fname) { return None; }
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "`{fname}` is provided by more than one loaded package — which one runs would be \
             decided by load order, so calling it is refused. Remove or rename one of them \
             (a stale copy of a renamed package in a libs dir is the usual cause); if both are \
             visible when compiling, the compiler reports this as E0601."
        ),
    ))
}

/// 类型侧孪生（`new` / 类型解析位）。判定与理由同 [`ambiguous_function_exception`]。
pub fn ambiguous_type_exception(
    ctx: &VmContext, module: &Module, class_name: &str,
) -> Option<crate::metadata::Value> {
    if !crate::metadata::lazy_loader::ambiguity_seen() { return None; }
    if !ctx.is_ambiguous_type(class_name) { return None; }
    Some(crate::exception::make_missing_symbol_exception(
        ctx, module,
        format!(
            "type `{class_name}` is provided by more than one loaded package — which one is \
             instantiated would be decided by load order, so using it is refused. Remove or \
             rename one of them; if both are visible when compiling, the compiler reports E0601."
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
