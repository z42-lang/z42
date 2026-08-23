# Spec: `[Deprecated]` built-in directive (PR5, D2)

> attribute-handler-registry 的 built-in directive 家族第一个**持久化**成员。分类为 directive
> （名字识别、无 backing 类、不进反射）。**采用零格式-bump 的 attr-ref 哨兵持久化**——复用已有的
> attr-ref blob 通道（class/method/field 三处 zbc 段早已能写 `IrAttrRef[]`，serde 先例），
> 以哨兵类型名 `$Deprecated` + msg 塞进 FactoryFunc 槽，**不 bump zbc/zpkg 格式**。
>
> **为什么零格式-bump（关键决策）**：格式-bump 路径在 CI 撞上**既有的两代自举回归**
> （gen1-stdlib z42.encoding `undefined String`，纯版本号 bump 的 PR #270 同样红 → 证明与本 PR
> 无关、是先存在的环境墙），且新跨成员符号会踩 F2 stale-cache（#247 DepScanCache path-only key）。
> 零格式-bump 同时绕开两者：不动格式 = 不触发两代自举转换；哨兵走既有 blob 通道 = 无新跨成员符号。
> 与 serde（#256）同款选择。

## ADDED Requirements

### Requirement: `[Deprecated]` 标注与持久化

#### Scenario: 方法标注 `[Deprecated]`（无消息）
- **WHEN** 某方法声明带 `[Deprecated]`
- **THEN** 编译器在该方法的 attr-ref 列表尾部追加一条哨兵 `IrAttrRef{ TypeName="$Deprecated", FactoryFunc="" }`
- **AND** 不合成任何 `DeprecatedAttribute` 反射工厂（directive，非 store-meta）；哨兵借用既有 attr-ref blob 通道，**不 bump 格式**

#### Scenario: 类标注 `[Deprecated("msg")]`
- **WHEN** 某类声明带 `[Deprecated("use Bar instead")]`
- **THEN** 编译器在该类的 attr-ref 列表追加 `IrAttrRef{ TypeName="$Deprecated", FactoryFunc="use Bar instead" }`（msg 塞进 FactoryFunc 槽）

#### Scenario: 字段标注 `[Deprecated]`
- **WHEN** 某字段（实例或静态）声明带 `[Deprecated]`
- **THEN** 编译器为该字段的 attr-ref 列表追加 `$Deprecated` 哨兵（字段 attr-ref blob 通道已存在，zbc1.14）

#### Scenario: 未标注符号零开销
- **WHEN** 符号未带 `[Deprecated]`
- **THEN** 不追加哨兵、attr-ref 列表原样；z42c/stdlib 全部符号如此 → **self-host gen1==gen2 逐字节一致**（本地实测 5/5）

### Requirement: use-site 弃用告警（含跨包）

#### Scenario: 调用被弃用方法 → 告警
- **WHEN** 代码调用一个 `[Deprecated("msg")]` 方法
- **THEN** 在调用点发一条 **Warning**（rule id `"deprecated"`），消息含被弃用符号名 + 作者提供的 msg
- **AND** 告警**不阻断编译**（severity=Warning，不增 ErrorCount）

#### Scenario: `new` 被弃用类 / 用其作类型 → 告警
- **WHEN** 代码 `new T()`、把 `T` 用作类型注解 / cast / `typeof` / `is`，而 `T` 带 `[Deprecated]`
- **THEN** 在引用点发 deprecated Warning（typecheck 相位 `_chkTypeRef` 命中 `Z42ClassType.IsDeprecated`）

#### Scenario: 跨 zpkg 引用被弃用的导入符号 → 告警
- **WHEN** 引用从另一个 zpkg 导入的 `[Deprecated]` 方法 / 类 / 字段
- **THEN** 告警照常触发——哨兵随 attr-ref blob 持久化，经 `IrDeprecation.Has/Msg` 在 TsigReconcile 读回 →
  `Exported*Z.IsDeprecated/DeprecationMsg` → ImportedSymbolLoader 还原到导入符号

### Requirement: use-site 告警治理（#suppress）

#### Scenario: `#suppress deprecated` 局部关闭
- **WHEN** 引用点被 `#suppress deprecated ... #restore` 字节区间覆盖
- **THEN** 该点的 deprecated 告警被抑制（复用 PR3c SuppressionSet 字节区间机制，按 rule id + 位置匹配）
- **实现**：TypeChecker.Infer 从 `cu.SuppressRegions` 构建 `_curSuppress`，AccessChecker `_recordDep`
  发告警前查 `_curSuppress.IsSuppressed("deprecated", sp.Start)`

## IR Mapping
- **零格式-bump**：不新增任何 zbc/zpkg 段或 flag 位。deprecated 状态借哨兵 `IrAttrRef` 走既有 attr-ref blob 通道：
  - class attrs：zbc1.10（TYPE 段已写 `IrAttrRef[]`）
  - method attrs：zbc1.11（SIGS 段已写 `IrAttrRef[]`）
  - field attrs：zbc1.14（TYPE 段已写实例 + 静态字段 `IrAttrRef[]`）
- 哨兵编码：`TypeName = "$Deprecated"`（`IrDeprecation.Sentinel`，`$` 前缀避免与真实用户 attr 类名冲突）；`FactoryFunc = msg`（借用工厂函数名槽承载消息串）
- 读回：`IrDeprecation.Has(attrs, count)` / `IrDeprecation.Msg(attrs, count)`（z42.ir，扫 attr-ref 数组匹配哨兵名）
- 反射：Rust `GetCustomAttributes` 未来应过滤 `$Deprecated` 哨兵（不暴露给用户反射）→ Deferred

## Pipeline Steps
- [x] Lexer — 无（`[X]` 已有词法）
- [x] Parser / AST — 无（`[Deprecated]` 走既有 attribute 语法；HandlerRegistry.IsDirectiveAttr 认 "Deprecated"）
- [x] IR Codegen — ClassDescBuilder `_attrRefs` 读 AST `[Deprecated]` → 追加 `$Deprecated` 哨兵到 attr-ref 列表
- [x] 持久化 — 复用既有 attr-ref blob（零格式-bump），无 writer/reader 改动
- [x] 跨包 — TsigReconcile（`IrDeprecation.Has/Msg`）→ Exported*Z（+IsDeprecated/DeprecationMsg）→ ImportedSymbolLoader → Symbol / Z42ClassType
- [x] 本地符号 — MemberCollector / StubCollector 从 `HandlerRegistry.HasDeprecated` 置 symbol.IsDeprecated
- [x] use-site — AccessChecker `CheckDeprecatedM/F/T` 在 MemberResolver（6 点）+ TypeChecker `_chkTypeRef`（类型引用）命中；`_recordDep` 直接经 `_tc._diags.Warning` 发告警（查 `_curSuppress` 抑制）
- [x] CompilerFingerprint 3→4（codegen 改变但无格式 bump，须 bump 指纹使缓存失效）
- [x] VM interp — 无执行语义变化（纯编译期告警 + 哨兵元数据；反射未暴露，Deferred）

## Deferred（本 PR 不做）
- **`[lints] deprecated = none` / `warnings-as-errors`**（z42.toml 治理）——deprecation 告警在 typecheck 相位
  直发 bag，接入 LintConfig 需把 `[lints]` 解析结果透到 typecheck；F2-安全但独立工作量，留后续 additive PR
- `[Suppress("deprecated")]` 声明子树抑制 deprecation 告警（需把 [Suppress] 活跃栈接入 typecheck 相位；`#suppress` 字节区间已支持）
- `[Deprecated(msg, isError=true)]`（C# `[Obsolete(msg,true)]` 硬错模式）→ 未来 additive
- 命名参数 `[Deprecated(message="...")]`
- deprecated 位镜像进反射 API（`MemberInfo.IsDeprecated`）；Rust `GetCustomAttributes` 过滤 `$Deprecated` 哨兵
- 属性（property）/ enum 成员级弃用（属性经 getter/setter 方法可间接覆盖；enum 成员 = 静态字段，随字段级能力已覆盖）
- **格式-bump 版持久化**（method_flags bit3 + TYPE-tail 弃用表）——待既有两代自举回归（PR #270 证）修复后可选迁移，收益仅"哨兵不占 attr-ref blob"，非必需
