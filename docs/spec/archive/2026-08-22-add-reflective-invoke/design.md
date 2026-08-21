# Design: 反射式调用补全（泛型方法 Invoke + 构造函数反射）

## 反射类型层级（对齐 C#）

```
MemberInfo
 └─ MethodBase              ← 新增：共享 Name/IsStatic/GetParameters()/__qualified（数据成员）
     ├─ MethodInfo          ← reparent：ReturnType/IsVirtual/泛型方法 API + Invoke→__method_invoke
     └─ ConstructorInfo     ← 新增：Invoke(object[])→__ctor_invoke（带参构造）
 └─ FieldInfo / PropertyInfo（不动）
```

**reparent 安全性**：z42 对象字段布局扁平，native 经 `read_obj_slot(mi, "__qualified")` 按槽名读，与声明
在 MethodInfo 还是 MethodBase 无关——现有 `builtin_method_invoke`/consumers 不受影响。共享成员上移到
MethodBase，`MethodInfo`/`ConstructorInfo` 继承即得。

**Invoke 契约**：C# 的 `MethodBase.Invoke` 抽象，两子类各实现。z42 侧**不**强求抽象方法——`MethodInfo` 与
`ConstructorInfo` 各带自己的 `extern Invoke`（分别绑 `__method_invoke` / `__ctor_invoke`），MethodBase 只承载
共享**数据成员**（Name/IsStatic/GetParameters/__qualified）。语义区别：MethodInfo.Invoke 调既有方法；
ConstructorInfo.Invoke 分配新实例 + 跑 ctor + 返 this。

## Architecture（泛型方法反射式调用）

```
定义态 MethodInfo  ──MakeGenericMethod(Type[])──►  构造态 MethodInfo
  (从 METHOD 元数据                                   (克隆 + 隐藏槽 __typeArgs=Type[])
   填 IsGenericMethod                                       │
   + 类型形参名)                                            │ Invoke(obj, args)
                                                            ▼
                                        builtin_method_invoke 读 __typeArgs
                                                            │ 非空 → 转类型名 Box<[String]>
                                                            ▼
                                        invoke_qualified(..., method_type_args)
                                                            │
                                                            ▼
                                        exec_function → frame.method_type_args = 实参名
                                                            │
                                                            ▼
                                方法体 M1 opcode MethodTypeArg/MethodDefault 物化
                                （typeof(T) / new T() / default(T)）
```

反射式调用**不新增执行路径**：它只是 `frame.method_type_args` 帧槽（M1 建）的第二个填充来源——
M1 直接调用由 `CallGeneric` 指令填，反射由 `__method_invoke` native 填，下游物化完全共用。

## 数据来源：方法级泛型元数据 —— **格式已预留，仅需 producer 填数（无 bump）**

**实施期关键发现**：zbc SIGS 段**每个函数签名早已含方法类型形参槽**，全链路 reader 已就绪，只 writer 恒写 0：

| 环节 | 现状 | 位置 |
|------|------|------|
| z42 writer | 恒写 `tpCount=0`（注「z42c IrFunction 无 typeparam → ZW-2 补」） | `ZbcWriter.z42:443` |
| z42 reader | **已读** tpCount + 每 tp 的名字 + 约束包（防御性，因恒 0 而空转） | `ZbcReader.z42:496-506` |
| Rust reader | **已读** → `FuncSig.type_params: Vec<String>` | `zbc_reader.rs:886-893`（struct :840） |

格式**自描述**：SIGS 条目里 `params` 之后是 `tpCount:u8 + [每 tp: nameIdx:u32 + 约束包]`，再 `attrCount:u16`。
tpCount 告知后随几个 → 填真实值**不改布局**（非泛型 tpCount=0 逐字节不变；泛型多出字节旧新 reader 都消费）。

**⚠️ 三个 SIGS reader 必须全部同步（CI 揭出的坑，2026-08-22）**：SIGS 的方法 tpCount + 约束包被
**三处**独立读取，改 writer 后必须逐一核对：① `ZbcReader.z42`（z42.ir 单模块 zbc，`:496` 循环有
`t++`✓）② Rust `zbc_reader.rs`（运行期，`for _ in 0..tp_count`✓）③ **`ZpkgReader.z42`（`Z42.Project`
命名空间，在 z42.ir 内，编译器 DepScan 用）**——第③处 `ReadModuleSigs` 的 tpc 跳过循环
**曾漏 `j = j + 1`**（tpc 恒 0 时无害；本变更让 tpc>0 后变**无限读越界** → `ZpkgCursor` 崩）。
**本地 GREEN gate（e2e/stdlib/self-host）不覆盖此路径**——它只在「DepScan 把*含泛型方法的 zpkg*
当依赖读」时触发（CI 的 test-asset 编译 + `xtask bench` build z42.scripting 才走到），故被 CI 而非
本地捕获。**教训：改 zbc/zpkg producer 必须 grep 全部 reader（含 z42.project ZpkgReader，非只
ZbcReader+Rust）+ 本地跑一次 DepScan 含新元数据的 zpkg（`xtask bench`）**。已修 j++ + bench 复现验证通过。

**producer 侧缺口（本变更填补）**：
1. `IrFunction`（`IrModule.z42`）加**方法级** `TypeParams: string[]` + `TypeParamCount`（IrModule.z42:4 原注「TypeParams 延后」）。
2. `FunctionEmitter.z42:47` 已算出 `md.TypeParams.Names/.Count`（M1 为调用点解析用）→ 存入 IrFunction 的方法级字段。
3. `ZbcWriter`：pre-pass intern 方法类型形参名（镜像类的 `:89`）；`:443` 硬编码 `0` → 真实 `tpCount + 名字 +
   空约束包`（`cflags=0, ifaceCount=0`，where 约束 Deferred）；约束包格式照抄类 writer `:269-290`。

**格式不 bump 的正当性**：① 布局自描述、非泛型 byte-identical；② 现有 stdlib/z42c 源**零泛型方法声明**
（grep 实测 0）→ 无任何现有方法字节变化 → 自举字节不动点 gen1==gen2 不受影响；③ 无版本号变 → 0.41 nightly
种子正常 warm build、CI 快路径、无两代自举。（符合 philosophy「不为破坏性顾虑牺牲最佳方案」——这里最佳方案恰是最小改动。）

## 反射侧数据流（FuncSig.type_params → MethodInfo）

`build_method_info`（`reflection.rs:602`）经 `resolve_func_sig` 从运行期 `Function`（`m.functions`）`extract`。
`FuncSig.type_params` 已读，但 `resolve_func_sig` 当前**不返回**它 → 需：① 运行期 `Function` 携带 type_params
（若未携带，thread `FuncSig.type_params`→`Function`，`bytecode.rs`/`merge.rs`）；② `resolve_func_sig` 返回它；
③ `build_method_info` 据此填 `IsGenericMethod`/`IsGenericMethodDefinition` + 类型形参名槽。

## Decisions

### Decision 1: 构造态表示 —— MethodInfo 挂隐藏 `__typeArgs`（参考 C#，无独立子类型）

**问题：** `MakeGenericMethod` 返回什么？如何携带绑定的类型实参？

**选项：**
- A — MethodInfo 加隐藏槽 `__typeArgs: Std.Type[]`，`MakeGenericMethod` 克隆 MethodInfo 盖该槽，
  返回同类型 `MethodInfo`。`Invoke` 读该槽填帧。
- B — 独立 `ConstructedGenericMethod` 子类型 + 独立 `__generic_method_invoke` native 路径。

**决定：** **A**。C# 的 `MakeGenericMethod` 正是返回 `MethodInfo`（无用户可见的独立构造态子类型——
`RuntimeMethodInfo` 内部承载 typeArgs，对外统一是 `MethodInfo`）。User 裁决「参考 C#（C# 无设计缺陷）」，
A 与 C# 语义一致，且最大化复用 M1 帧槽 + 现有 `invoke_qualified`，改动面最小、与直接调用逐点一致。
B 语义更「显式」但与 C# 不符、且分岔 M1 路径，弃。

### Decision 2: 类型实参在 native 层的表示 —— 转类型名 `Box<[String]>` 复用 M1 帧槽

**问题：** `Std.Type[]` typeArgs 如何进 `frame.method_type_args`（M1 定义为 `Box<[String]>`，存 FQ 类型名）？

**决定：** `builtin_method_invoke` 把 `__typeArgs` 的每个 `Std.Type` 取其 FQ 名（复用 reflection.rs 现有
`__type_full_name` 逻辑），组成 `Box<[String]>`，经改造后的 `invoke_qualified` 线程进
`exec_function` → `frame.method_type_args`。这与 M1 `CallGeneric` 编进指令的类型名字符串**同构**，
下游 `MethodTypeArg`/`MethodDefault`/`__activator_create` 物化零改动。

**注意（根因修复）：** `invoke_qualified` 当前调 `exec_function`（无类型实参参数）。需给它加
`method_type_args: &[String]` 参数并传到一个填帧的 exec 变体（`mod.rs` 已有
`exec_function_from_regs(..., method_type_args)` 填帧模式，参照它给 `exec_function` 加同款线程，或让
`invoke_qualified` 直接走填帧路径）。**不**在消费端加「if 泛型」特例分支——统一线程，空切片即 M1 的空帧
（byte-identical 非泛型行为）。

### Decision 3: arity / 泛型性校验位置 —— MakeGenericMethod native 层

**问题：** typeArgs 个数与方法 arity 不符、或对非泛型方法调 MakeGenericMethod，在哪拦？

**决定：** `__method_make_generic` native 内校验：读 MethodInfo 的 `IsGenericMethod` + 类型形参数
（来自新 METHOD 元数据）；非泛型 → 抛 catchable `Std.Exception`；`typeArgs.len != TpCount` → 抛
catchable `Std.Exception`（消息含期望/实际）。经 `ctx.set_pending_thrown` 走 catchable 通道
（与非泛型 Invoke arity 检查 `invoke_arity_check` 同款）。

### Decision 4: 单 PR 交付（含格式 bump）

**问题：** 元数据+格式 bump 与反射消费是否拆多 PR / 多 nightly？

**决定：** 单 PR（User 裁决）。理由：新 METHOD 元数据由 **fresh z42c**（本 PR 的 z42.ir）emit、由 **fresh
runtime**（本 PR 的 zbc_reader）读；z42c 源 / xtask 源均**不**使用该反射 API（serde 才用，另开）→ 不触
bootstrap-seed.md 轴②/③的「晚一 nightly」。格式 bump 由 CI 两代自举吸收（#242 已修 region.rs u16 溢出
真凶，两代路径现可用）。

### Decision 5: ConstructorInfo.Invoke —— 复用 ctor 函数 + 分配，重开带参构造

**问题：** `ConstructorInfo.Invoke(args)` 如何跑构造函数？带参构造此前 Deferred（`Activator.CreateInstance`
只分配不跑 ctor）。

**选项：**
- A — 分配实例（同 `__activator_create` 的 alloc + typeArgs 具化）+ 以新对象为 receiver(reg0) 经
  `invoke_qualified` 跑 ctor 函数（`<ClassFQ>.<ClassSimpleName>[$N]`，已在方法表）+ 返回该对象。
- B — 新增专门的「带参构造」执行路径（不复用 invoke_qualified）。

**决定：** **A**。ctor 在 z42 就是方法表里的普通函数（`new C(args)` 由 `ObjNewInstr` = alloc + call
CtorName 实现，`IrInstr.z42:426`），`ConstructorInfo.Invoke` 只是把这套「alloc + call-with-this」搬到反射
层：`__ctor_invoke(ci, args)` 读 `__qualified`（ctor 函数 FQ 名）→ alloc 实例 → `invoke_qualified` 以新对象
reg0 + args 跑 → 返回对象。arity 校验同 Decision 3 走 catchable。**复用最大化**，无新执行路径。

> **注意**：这重开了此前 Deferred 的「带参构造」能力（`reflection.rs:2146`）。设计上不改
> `Activator.CreateInstance(Type)`（保持无参快路径、不跑 ctor），带参构造只经 ConstructorInfo，职责分明
> （见 constructor-reflection spec 的 MODIFIED Requirement）。

### Decision 6: GetConstructors 枚举 —— ctor 命名约定，无格式字段

**问题：** 如何识别一个类型的构造函数？

**决定：** 按 ctor 命名约定枚举——方法表中 FQ 名匹配 `<ClassFQ>.<ClassSimpleName>`（含 `$N` 重载后缀）者
即 ctor（`ObjNewInstr.CtorName` 就是这么解析的）。**无需**新增 is-ctor 元数据位 / 格式字段。若 IMPL 期
发现命名约定脆弱（如用户方法名恰与类同名——C# 禁止但需确认 z42 是否也禁），再评估把 is-ctor 位搭 0.42
同次 METHOD 段 bump（不额外 bump）。

## Implementation Notes

- **METHOD 段 ABI 前缀不变**：新字段追加在 params 之后，旧种子读旧 zbc 的前缀解析不受影响（但格式
  strict-pin 会因 minor 变而整体拒读旧 zbc——这是预期，pre-1.0 不兼容旧产物，残留 fixture 重生）。
- **MethodInfo 构造点**：`__type_methods`（vtable 方法）与其它 MethodInfo 构造处都需从 METHOD 元数据
  填泛型形参名；`IsGenericMethod` 派生自「TpCount>0 或 __typeArgs 非空」，`IsGenericMethodDefinition`
  派生自「TpCount>0 且 __typeArgs 空」。
- **`GetGenericArguments()`**：定义态从类型形参名构造占位 `Std.Type`（复用类类型形参占位机制，见
  reflection.rs:953 `GetGenericArguments` 对 Type 的既有实现）；构造态直接返回 `__typeArgs`。
- **实例 vs 静态**：`Invoke` 已按 `IsStatic` 决定是否 push receiver（reflection.rs:1666），泛型线程与之
  正交——两条路径统一经 `invoke_qualified` 加 `method_type_args`。
- **JIT**：M1 已让 `jit_unsupported_reason` 对含方法级泛型 / 泛型调用点的函数 interp-fallback；反射式
  调用经 native 进 interp `exec_function`，无需 JIT 改动。

## Deferred / Future Work

### generic-method-invoke-future-constraint-check
- **来源**：本 spec Out of Scope
- **触发原因**：反射式绕过编译期 `ConstraintChecker`；运行期 where 约束校验需额外元数据 + 运行期类型关系判定
- **前置依赖**：方法 where 约束元数据 emit 到 METHOD 段
- **触发条件**：serde 或用户反射代码需要「MakeGenericMethod 时校验实参满足 where」时
- **当前 workaround**：直接调用路径（M1）仍有编译期约束校验；反射式不校验（信任调用方）

### generic-method-invoke-future-open-generic-cross
- **来源**：本 spec Out of Scope
- **触发原因**：开放泛型类上的泛型方法，类形参 + 方法形参双层开放态的组合具化未定义
- **前置依赖**：类型实参解析统一处理双层来源
- **触发条件**：反射调用「泛型类的泛型方法」且类形参也需运行期绑定时
- **当前 workaround**：先 `MakeGenericType` 具化类，再在构造类上取方法 `MakeGenericMethod`

### ctor-reflection-future-overload-resolution
- **来源**：本 spec Out of Scope（constructor-reflection）
- **触发原因**：`GetConstructor(Std.Type[])` 按参数类型匹配重载是独立复杂度（隐式转换、最佳匹配规则）
- **前置依赖**：运行期参数类型 ↔ ctor 形参类型的匹配/best-match 逻辑
- **触发条件**：调用方需要「按参数类型直接拿某个 ctor」而非枚举全部自选时
- **当前 workaround**：`GetConstructors()` + 逐个 `GetParameters()` 检查，调用方自选目标 ctor

## Testing Strategy

- **单元（z42.core [Test]）**：
  - 泛型方法：`IsGenericMethod`/`IsGenericMethodDefinition`/`GetGenericArguments` 定义态与构造态；
    `MakeGenericMethod` 成功 / arity 错 / 非泛型错。
  - 构造函数：`GetConstructors` 枚举（带参/无参/多重载）；`MethodBase` 层级关系（MethodInfo/ConstructorInfo
    是 MethodBase，ConstructorInfo 非 MethodInfo）。
- **Golden**：
  - `src/tests/generic-method-invoke/`：静态 Invoke 返回值；实例 Invoke；`typeof(T)` 反射==直接；
    `default(T)` 值/引用；`new T()` GetType；反射 throw 保留类型。
  - `src/tests/ctor-reflection/`：GetConstructors 枚举；带参 `Invoke` 建实例（字段已初始化）；无参 ctor；
    arity 错抛异常；ctor 内 throw 保留类型。
- **格式**：`zbc_reader_tests.rs` pinned 37/42；fixture header 重生。
- **VM 验证**：完整 `xtask test` GREEN gate；`xtask test compiler` 自举字节不动点（gen1==gen2）;
  `xtask test bootstrap` 无越界（确认 z42c/xtask 源未越界用新格式/API）。
