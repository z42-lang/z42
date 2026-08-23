# Spec: `[Deprecated]` built-in directive (PR5, D2)

> attribute-handler-registry 的 built-in directive 家族第一个**持久化**成员。分类为 directive
> （名字识别、无 backing 类、不进反射），但与 `[Suppress]`（纯编译期）不同——它**烘进 descriptor**
> （flag + 消息串），跨包可见，故须格式 bump（zbc 1.36→1.37 / zpkg 0.41→0.42）。

## ADDED Requirements

### Requirement: `[Deprecated]` 标注与持久化

#### Scenario: 方法标注 `[Deprecated]`（无消息）
- **WHEN** 某方法声明带 `[Deprecated]`
- **THEN** 编译器把「deprecated 位」烘进该方法的 SIGS descriptor（method_flags 新 bit），消息串为空
- **AND** 不合成任何 `DeprecatedAttribute` 反射工厂、不写 IrAttrRef blob（directive，非 store-meta）

#### Scenario: 类标注 `[Deprecated("msg")]`
- **WHEN** 某类声明带 `[Deprecated("use Bar instead")]`
- **THEN** 编译器把「deprecated 位 + 消息池索引」烘进该类的 TYPE descriptor（class flags 新 bit + msg idx）

#### Scenario: 字段标注 `[Deprecated]`
- **WHEN** 某字段（实例或静态）声明带 `[Deprecated]`
- **THEN** 编译器为该字段 descriptor 新增的 field-flags 字节写入「deprecated 位」（+ 可选消息池索引）
- **注**：字段 descriptor 当前无 flags 字节——本 PR 为实例 + 静态字段循环各新增一个 field-flags `u8`

#### Scenario: 未标注符号零开销
- **WHEN** 符号未带 `[Deprecated]`
- **THEN** 其 deprecated 位为 0、消息池索引为空哨兵；z42c/stdlib 全部符号如此 → **self-host gen1==gen2 逐字节一致**（新格式下）

### Requirement: use-site 弃用告警（含跨包）

#### Scenario: 调用被弃用方法 → 告警
- **WHEN** 代码调用一个 `[Deprecated("msg")]` 方法
- **THEN** 在调用点发一条 **Warning**（rule id `"deprecated"`），消息含被弃用符号名 + 作者提供的 msg
- **AND** 告警**不阻断编译**（severity=Warning，不增 ErrorCount）

#### Scenario: `new` 被弃用类 / 用其作类型 → 告警
- **WHEN** 代码 `new T()`、把 `T` 用作类型注解 / cast / `typeof` / `is`，而 `T` 带 `[Deprecated]`
- **THEN** 在引用点发 deprecated Warning

#### Scenario: 跨 zpkg 引用被弃用的导入符号 → 告警
- **WHEN** 引用从另一个 zpkg 导入的 `[Deprecated]` 方法 / 类 / 字段
- **THEN** 告警照常触发——deprecated 位随 descriptor 持久化、经 TsigReconcile → Exported*Z → ImportedSymbolLoader 还原到导入符号

### Requirement: 告警治理（[lints] / warnings-as-errors / #suppress / [Suppress]）

#### Scenario: `[lints] deprecated = none` 抑制
- **WHEN** 消费方 `z42.toml` 有 `[lints]` 段 `deprecated = "none"`
- **THEN** 该编译单元内所有 deprecated 告警被抑制、不进 bag

#### Scenario: `warnings-as-errors` 升级
- **WHEN** `[lints]` 段 `warnings-as-errors = true`
- **THEN** deprecated 告警升为 Error、增 ErrorCount、阻断编译

#### Scenario: `#suppress deprecated` 局部关闭
- **WHEN** 引用点被 `#suppress deprecated ... #restore` 字节区间覆盖
- **THEN** 该点的 deprecated 告警被抑制（复用 PR3c SuppressionSet 字节区间机制，按 rule id + 位置匹配）
- **注**：`[Suppress("deprecated")]` 声明子树抑制**本 PR 不覆盖**——它靠 walk 期活跃栈（decl.Span start-only），而 deprecation 检测在 typecheck 相位、治理在其后（相位错配）；留 Deferred（见文末）

## IR Mapping
- method: SIGS `method_flags` u8 新增 bit3=deprecated（bit2=sealed 先例）+ 置位时 SIGS 条目尾部追加 msg 池索引 `u32`（gated）
- class/field: **TYPE 段体尾部 size-gated 弃用表**（class_flags 已满、bit6=delegate；per-field 非-gated 字节破两代自举）——
  `classDepCount:u16 + (classIdx:u16, msgIdx:u32)× + fieldDepCount:u16 + (classIdx:u16, isStatic:u8, fieldIdx:u16, msgIdx:u32)×`，
  仅在本模块有弃用时写（未弃用零字节、与旧格式一致 → 两代自举安全；reader 靠 cursor<TYPE 段末 判存在）
- 格式：zbc `ZbcVersion.Minor` 36→37；zpkg `ZpkgWriterZ.Minor` 41→42（联动，MODS 段面不变）

## Pipeline Steps
- [x] Lexer — 无（`[X]` 已有词法）
- [x] Parser / AST — 无（`[Deprecated]` 走既有 attribute 语法）
- [ ] TypeChecker — use-site `AccessChecker.CheckDeprecated` 收集 hit
- [ ] IR Codegen — IrGen/ClassDescBuilder 读 attr → 烘 flag + msg
- [ ] zbc/zpkg writer + 3 readers — flag + msg 序列化/反序列化（格式 bump）
- [ ] 跨包 — TsigReconcile / Exported*Z / ImportedSymbolLoader / Symbol / Z42ClassType
- [ ] 治理 — `_runDeprecation` pass 经 DiagSinkImpl + LintConfig + SuppressionSet 发告警
- [ ] VM interp — 无执行语义变化（纯编译期告警 + 元数据位；反射不暴露，Deferred）

## Deferred（本 PR 不做）
- `[Suppress("deprecated")]` 声明子树抑制 deprecation 告警（需把 [Suppress] 活跃栈接入 typecheck 相位；`#suppress` 字节区间已支持）
- `[Deprecated(msg, isError=true)]`（C# `[Obsolete(msg,true)]` 硬错模式）→ 未来 additive
- 命名参数 `[Deprecated(message="...")]`
- deprecated 位镜像进反射 API（`MemberInfo.IsDeprecated`）
- 属性（property）/ enum 成员级弃用（属性经 getter/setter 方法可间接覆盖；enum 成员 = 静态字段，随字段级能力已覆盖）
