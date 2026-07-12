# Design: 统一类型元数据（unify-type-metadata）

## Architecture — 从"两份"到"一份反射级元数据"

```
现状（两份，编译期那份是运行时那份的超集副本）:
  ┌─ MODS ─────────────┐        ┌─ TSIG / IMPL / EXPT ──┐
  │ TYPE (ClassDesc)   │        │ 导出类型接口(全签名)  │
  │ SIGS (FuncSig)     │        │ 含可见性/virtual/     │
  │  → VM 执行 + 反射  │        │  minArg/enum值/       │
  └────────────────────┘        │  delegate/impl        │
                                │  → z42c 编译期        │
                                └───────────────────────┘
       ↑ 反射读这份(不完整)          ↑ z42c 读这份(完整但重复)

终态(一份，反射级完整，两个消费者):
  ┌─ MODS ───────────────────────────────────┐    ┌─ impls(极小) ─┐
  │ TYPE: +字段可见性 +方法(可见性/virtual/  │    │ 跨包 impl 关联 │
  │        abstract/minArg/默认值/varargs)   │    │ (target,trait, │
  │       +enum成员值 +delegate Invoke        │    │  args,methods) │
  │ SIGS: +可见性/virtual/minArg/默认/varargs │    └───────────────┘
  │       +参数名                             │           ↑
  └───────────────────────────────────────────┘    两个消费者都读
        ↑ VM(执行+反射) 与 z42c(编译) 读同一份    z42c 编译合并 + VM 反射/派发
  删除: TSIG(折进上面) + EXPT(可见性派生)
```

**原则(吸收 C# / 避其复杂度）**:C#/.NET 一份 ECMA-335 元数据同时服务编译引用 + 反射 + JIT,
用 Field/Method flags 存可见性/virtual、用 Constant 表存 enum 值 + 参数默认值。**吸收**其"单一
真相源 + 富 flag + 常量值";**避开**其 ~40 张互链表 + heap + RID token 的复杂度——**z42 保留扁平
命名段 + FQ 名引用**,只是把缺的字段 additive 加进现有 TYPE/SIGS。

## Decisions

### D1: TSIG → TYPE/SIGS 逐字段映射（终态每样都有家）

| TSIG 现有字段 | 终态归宿 | 现状 | P1 动作 |
|---|---|---|---|
| 类名/基类/type-params/约束/class flags/接口/attributes | **TYPE.ClassDesc**（已有） | ✅ | — |
| 字段 名/类型 | **TYPE.FieldDesc**（已有） | ✅ | — |
| 字段 **可见性** | TYPE.FieldDesc **+visibility:u8** | ❌ | 加 |
| 字段 is_static | 已由 static_fields 分块表达 | ✅ | — |
| 函数/方法 名/参数类型/返回/is_static/type-params/约束/attributes/param-attrs | **SIGS.FuncSig**（已有） | ✅ | — |
| 方法 **可见性** | SIGS **+visibility:u8** | ❌ | 加 |
| 方法 **virtual/abstract** | SIGS **+method_flags:u8**（bit: virtual/abstract；static 已有） | ❌ | 加 |
| **minArgCount**（默认参数个数） | SIGS **+min_arg:u16** | ❌ | 加 |
| **参数默认值**（Constant，C# 式） | SIGS 每参 **+has_default:bit + default_const** | ❌ | 加（可 P1b 分批） |
| **paramsFrom**（varargs） | SIGS **+params_from:u8**（0xFF=无） | ❌ | 加 |
| **参数名**（named args / 反射 ParameterInfo.Name） | SIGS 每参 **+name_str_idx**（现反射从 DBUG 猜） | ❌ | 加 |
| **enum 类型 + 成员值** | TYPE **enum 表**（class_flags bit5=enum + 成员 {name,i64 value}） | ❌ | **P1 第一砖** |
| **delegate 签名** | TYPE **delegate-as-class**（含 Invoke 方法签名，见 D5） | ❌ | 加 |
| **impl（跨包 impl Trait for Type）** | **impls 表保留**（见 D2） | IMPL 段（编译期）→ 统一元数据 | P1 让 VM 也读 |
| **EXPT（导出符号清单）** | **删**——由 TYPE/SIGS 里 public 可见性项派生 | EXPT 段 | P3 删 |

> 每个"加"都是 additive zbc format bump（旧字段不动、末尾追加,老 reader 读不到新字段但不崩;
> strict-pin 下正常 regen)。反射同步暴露该字段 → 每个 bump 自带反射测试。

### D2: impl 设计（irreducible，保留但重新定性为统一元数据）

**问题**:`impl TraitT for TypeA`——impl 声明在包 B,TypeA 在包 A,TraitT 可能在包 C。这个
"跨包关联"不属于任何单方的 TYPE(A 的 TYPE 写时不知道 B 的 impl)。C# 没有 orphan impl,帮不上。

**现状机制**（已查证）:
- **编译期**:z42c `_extractImpls`（生产端 B 提取）→ IMPL 段;消费端 `_mergeImpl` 把 impl 方法
  并入 imported TypeA 的方法集 + TraitT 入 TypeA 接口集(仅当 TypeA 在 import 集)。
- **运行时**:VM **不读 IMPL**。跨包 impl 方法作为全局函数,vcall 走 vtable `(方法名→FQ函数名)`
  + `resolve_virtual`/`func_index` 按名 3-way fallback 解析。**能派发,但 `TypeA.GetInterfaces()`
  反射不到 B 加的 TraitT**——反射缺口。

**决定**:**保留 impls 表,但从"编译期专用 IMPL 段"重定性为"统一元数据的一部分,z42c + VM 都读"**:

```
impls 表（每包存"本包声明的 impl"）:
  count
  每条 { target_fq_idx, trait_fq_idx, type_arg_idx[], method[]{name,sig,flags} }
```

- **z42c 编译期**:同现在(`_mergeImpl` 合并),读取源从 TSIG-内嵌-IMPL 改为读这张表。
- **VM 载入期**:新增"跨包 impl fixup"——载入包 B 时,把 B 的 impls 合并进已载入 TypeA 的
  `TypeDesc`:①`interfaces` 加 TraitT（→ `GetInterfaces()` 反射到跨包 trait ✓);②方法并入
  target 的可反射方法集 + vtable（→ 跨包 impl 方法可反射 + 派发更 robust，不再纯靠 name-fallback)。
  VM 已有 cross-zpkg fixup 基建(loader.rs)。
- **为什么不折进 TYPE**:target 的 TYPE 在**另一个包**,写时不可能含下游 impl → impl 必须随
  **声明它的包**走。这是 irreducible 的,不是冗余。它极小(z42.core 0.1%),且**它进统一元数据后
  变成反射特性**(跨包 impl 接口反射),不是负担。

**命名**:段名可留 `IMPL`（语义已变为"统一 impls 表"),或改 `IMPLS`;倾向留 `IMPL` 避免 churn,
在 zbc.md 注明其消费者从"仅 z42c"变为"z42c + VM"。

### D3: EXPT 删除——可见性派生导出面

EXPT 现在存"本包导出哪些符号(名+kind)"。终态:一个符号是否导出 = 它在 TYPE/SIGS 里的
**可见性是 public**。z42c 跨包解析时遍历 dep 的 TYPE/SIGS,按 public 筛出导出面 → EXPT 冗余 → P3 删。
（前提:D1 的可见性字段已落 TYPE/SIGS。)

### D4: 运行时"导出面"筛选 = 反射的 public 过滤，一鱼两吃

z42c 需要"这个 dep 导出了哪些 public 类型/成员"。VM 反射也需要"public vs 非 public"
（`Type.GetMethods()` 默认只返 public;`BindingFlags.NonPublic` 才含私有)。**同一个可见性字段
同时服务两者** → D1 加可见性不是"为编译期加",是补反射 + 顺带让 EXPT 可删。

### D5: delegate 表示——带 Invoke 的特殊类（C# 式）

C# 里 delegate 是继承 `MulticastDelegate` 的类,带一个 `Invoke` 方法承载签名,反射
`delegateType.GetMethod("Invoke")` 拿签名。**决定采同款**:delegate 在 TYPE 里作 class_flags
标记的特殊类,其签名 = 一个合成 Invoke 方法(存进该类方法表)。→ z42c 跨包 delegate 类型解析
+ VM delegate 反射,共用一条。避免为 delegate 单开一张表。

### D6: 三阶段 → 具体 change 拆分

**P1 超集（多个 additive change，各自 support 先行晚一 nightly 再 use）**:
- **P1-a `add-enum-type-metadata`**（首个可实施，= roadmap 0.3.12 IsEnum):enum 成员值进 TYPE
  + `Type.IsEnum` / `Enum.GetValues()` / `GetNames()` 反射。zbc additive bump。
- **P1-b `add-member-visibility`**:字段+方法可见性进 TYPE/SIGS + `FieldInfo.IsPublic`/
  `MethodInfo.IsPublic` 等反射。
- **P1-c `add-method-modifiers`**:方法 virtual/abstract flags → `MethodInfo.IsVirtual/IsAbstract`。
- **P1-d `add-param-metadata`**:minArg + 默认值 + varargs + 参数名 → `ParameterInfo.IsOptional/
  DefaultValue/Name`、`params` 反射。
- **P1-e `add-delegate-metadata`**（若跨包 delegate 需要）+ **`add-crosspkg-impl-reflection`**
  （VM 读 impls → `GetInterfaces` 含跨包 trait）。

**P2 对账 `reconcile-tsig-from-runtime-sections`**:z42c 新增"从 TYPE/SIGS/impls 重建
ExportedModuleZ"的路径,与现有 TSIG 读取**并行跑 + 逐字段 assert 相等**（TSIG 当 oracle)。
CI 加一条 reconcile gate。零行为变化(仍用 TSIG 结果,重建只对账)。

**P3 删 `drop-tsig-expt`**:切 z42c 读取源到重建路径、删 TSIG+EXPT emit、删 TSIG 段。zpkg
format bump（段面减两段)。每个 zpkg 变小。impls 段保留(D2)。

### D7: 为什么这个顺序安全

- P1 纯 additive + 反射测试全覆盖 → 低风险,且**每步立刻交付 roadmap C 流反射价值**。
- P2 有 TSIG 当 oracle 对账 → 重建正确性在删之前被证明（我们刚吃过"编译期解析 bug 极难调"的亏,
  这条对账 gate 是专门的安全网)。
- P3 只在对账干净后删 → 切换点风险已被 P2 前置消除。
- 全程 support-先行/晚一 nightly（bootstrap-seed.md),两代自举吸收每次 format bump（已验证可用)。

## Implementation Notes

- **字段编码**:可见性/flags 用 u8 bitfield;enum 值 i64;默认值走类似 Constant 的
  `has_default:bit + typed const`;参数名 str_idx。均追加在各记录**末尾**,保 additive。
- **z42c 侧重建**（P2）:`ExportedTypeExtractor` 现在从 AST 提取;P2 加一条"从 dep 的 TYPE/SIGS
  bytes 提取 ExportedModuleZ"的路径（`ZpkgReader` 读 TYPE/SIGS → 组装),与 `ReadTsig` 结果对账。
- **VM impl fixup**（D2）:复用 loader.rs 现有 cross-zpkg TypeDesc fixup 时机,载入含 impls 的包时
  合并进 target。注意 target 可能**尚未载入**（lazy)→ 需延迟合并/懒 fixup（VM 已有 UNRESOLVED
  懒解析基建)。
- **可见性默认**:老 fixture regen 后所有成员按其真实可见性写;VM 反射默认过滤器对齐 C#
  （GetMethods 默认 public+instance)。

## Testing Strategy

- **P1 每 change**:反射单测（新 API 真实返回值,非 stub)+ golden + 全 GREEN + 自举不动点。
  enum/impl 反射用 cross-zpkg fixture（impl 在 B、type 在 A → GetInterfaces 反射到)。
- **P2**:reconcile gate——对全 22 stdlib + z42c 7 包,重建的 ExportedModuleZ 与 TSIG 逐字段
  相等(byte/结构级)。任一不等即红,暴露重建缺口。
- **P3**:删后全 GREEN + 自举不动点 + 每 zpkg 尺寸下降量记录;cross-zpkg/impl/反射端到端不回归。
- 每阶段 format bump 走两代自举(本地端到端 + CI verify-selfhost)。

## Deferred / Future Work

### unify-metadata-future-reference-assembly
- **来源**:本 design Out of Scope
- **触发原因**:C# reference-assembly（剥 MODS 方法体、留元数据）是**正交**维度,与"删 TSIG 重复"
  无关;先做完统一再评估。
- **前置依赖**:本 initiative P3 完成（元数据已单一化)。
- **触发条件**:需要"编译-only 依赖分发"(比运行时包更小,只供编译引用)时。
- **当前 workaround**:无需求;运行时包已含完整元数据可编译引用。
