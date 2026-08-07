# Spec: sealed 修饰符（语义强制 + 简写 + 去虚化）

## ADDED Requirements

### Requirement: 禁止继承 sealed 类

#### Scenario: 类以 sealed 类为基类
- **WHEN** 编译 `sealed class A {}` 后出现 `class B : A {}`
- **THEN** 报编译错误「cannot inherit from sealed class 'A'」，编译失败（非警告）

#### Scenario: sealed 类实现接口仍合法
- **WHEN** `sealed class A : IFoo {}`（`IFoo` 是接口）
- **THEN** 编译通过——sealed 只禁止被**类继承**，不禁止实现接口

#### Scenario: 跨包继承导入的 sealed 类
- **WHEN** `B` 在 pkgB 中 `class B : A`，而 `A` 是 pkgA 导出的 `sealed class`
- **THEN** 报同样的继承错误（`ImportedSymbolLoader` 从 `CLASS_FLAG_SEALED` 还原 sealed 性）

### Requirement: 禁止 override sealed 方法

#### Scenario: 子类 override 一个 sealed override 方法
- **WHEN** 基类链中某方法为 `sealed override`（或方法级 sealed），子类再写 `override` 同签名方法
- **THEN** 报编译错误「cannot override sealed method 'M'」

#### Scenario: override 普通 virtual 方法不受影响
- **WHEN** 基类方法是 `virtual`（未 sealed），子类 `override`
- **THEN** 编译通过（现有行为不变）

### Requirement: 方法上 sealed 作为 sealed override 的简写

#### Scenario: 方法单写 sealed，基类有匹配 virtual
- **WHEN** 基类有 `virtual void M()`，子类写 `sealed void M()`（无 `override`）
- **THEN** 等价于 `sealed override void M()`：绑定为对基类 M 的 override（vtable 槽对齐），并置方法 sealed 位；后续子类不得再 override M

#### Scenario: 显式 sealed override 仍合法（C# 兼容）
- **WHEN** 子类写 `sealed override void M()`
- **THEN** 与单写 `sealed` 语义完全一致；`override` 视为允许的冗余，不报错

#### Scenario: 方法 sealed 但无匹配基类 virtual
- **WHEN** 方法写 `sealed`（或 `sealed override`）但基类链中没有可 override 的同签名 virtual 方法
- **THEN** 报编译错误「sealed method 'M' must override a base virtual method」——封死一个非 override 方法无意义

### Requirement: 方法 sealed 位进入元数据与反射

#### Scenario: sealed 方法的 method_flags 带 sealed 位
- **WHEN** 编译一个方法级 sealed 的方法
- **THEN** 其 SIGS `method_flags` 置 `METHOD_FLAG_SEALED`（bit2）；zbc header minor = 1.30

#### Scenario: 反射查询方法 sealed 性
- **WHEN** 运行期对该方法的 `MethodInfo` 调 `IsSealed`
- **THEN** 返回 `true`；非 sealed 方法返回 `false`

> **去虚化（原 ④）已拆为 follow-up `add-sealed-devirt`**——不在本 spec。本 change 落齐其地基
> （class/method sealed 位 + `Z42ClassType.IsSealed`/`MethodSymbol.IsSealed` 本地+跨包）。

### Requirement: 跨包 sealed 强制

#### Scenario: 继承导入的 sealed 类
- **WHEN** `B` 在 pkgB `class B : A`，`A` 是 pkgA 导出的 `sealed class`
- **THEN** 报继承错误（`ImportedSymbolLoader` 从 `ExportedClassZ.IsSealed` 还原，源自 TYPE `CLASS_FLAG_SEALED`）

#### Scenario: override 导入的 sealed 方法
- **WHEN** 子类 override 一个 pkgA 导出的方法级 sealed 方法
- **THEN** 报 override 错误（`ExportedMethodZ.IsSealed` 源自 SIGS `METHOD_FLAG_SEALED`）

## MODIFIED Requirements

### Requirement: method_flags 位布局

**Before:** `method_flags:u8` 仅定义 bit0=virtual、bit1=abstract（zbc 1.24）。

**After:** 新增 bit2=sealed（zbc 1.30）。bit0/bit1 语义不变；布局仍为单 u8（字节宽度不变，仅新增一个先前保留为 0 的位的语义）。

## IR Mapping

- 方法 sealed 性 → SIGS `method_flags` bit2（`METHOD_FLAG_SEALED`）。
- 类 sealed 性 → 沿用既有 `CLASS_FLAG_SEALED`（class-shape flags bit1，zbc 1.12，不新增）。
- 跨包 → TSIG `ExportedClassZ.IsSealed` / `ExportedMethodZ.IsSealed`（从上述两 flag 提取，无新增序列化）。

## Pipeline Steps

受影响的 pipeline 阶段（按顺序）：

- [x] Lexer —— 无改动（`sealed` token 已存在）
- [x] Parser / AST —— 无改动（`sealed` 已被 `_isModifier` 接受、收进 `Mods`）
- [ ] TypeChecker / SymbolCollector —— ① 继承/override 强制、③ shorthand override 解析、标 `IsSealed`（本地 + 跨包）
- [ ] IR Codegen —— ② `_methodFlags` sealed 位
- [ ] 元数据序列化 —— zbc/zpkg minor bump（writer 端）+ TSIG 提取
- [ ] VM interp —— reader 读 minor 1.30；`MethodInfo.IsSealed` 反射
