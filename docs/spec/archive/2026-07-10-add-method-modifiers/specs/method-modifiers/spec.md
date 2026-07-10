# Spec: 方法修饰符反射

## ADDED Requirements

### Requirement: SIGS 持久化 method_flags

#### Scenario: virtual 方法
- **WHEN** z42c 编译一个 `virtual` 方法
- **THEN** 其 SIGS 条目 `method_flags` bit0 置位（bit1 清）

#### Scenario: abstract 方法
- **WHEN** z42c 编译一个 `abstract` 方法
- **THEN** `method_flags` bit0 **且** bit1 均置位（abstract ⊂ virtual）

#### Scenario: override 方法
- **WHEN** z42c 编译一个 `override` 方法
- **THEN** `method_flags` bit0 置位、bit1 清

#### Scenario: 普通（非虚）方法 / 自由函数
- **WHEN** z42c 编译无 virtual/override/abstract 修饰的方法或自由函数
- **THEN** `method_flags` = 0

### Requirement: MethodInfo 暴露 IsVirtual / IsAbstract（源自 flag）

#### Scenario: IsVirtual 权威化
- **WHEN** 反射 `typeof(C).GetMethods()` 取到一个 `virtual`/`override`/`abstract` 方法
- **THEN** `MethodInfo.IsVirtual == true`

#### Scenario: IsAbstract
- **WHEN** 反射取到一个 `abstract` 方法
- **THEN** `MethodInfo.IsAbstract == true`（且 `IsVirtual == true`）

#### Scenario: 非虚方法
- **WHEN** 反射取到一个普通方法
- **THEN** `IsVirtual == false` 且 `IsAbstract == false`

## MODIFIED Requirements

**Before:** `MethodInfo.IsVirtual` 由「方法是否来自 vtable 迭代」的运行期启发式给出。
**After:** `MethodInfo.IsVirtual` 源自 SIGS `method_flags` bit0（声明修饰符权威）；vtable-presence
仅作 flag 缺失时的回退。新增 `MethodInfo.IsAbstract`（源自 bit1）。

## IR Mapping

- SIGS section 每 `FuncSig` 追加 `method_flags:u8`（bit0 virtual / bit1 abstract），紧接
  `visibility:u8` 之后、`param_types` 之前。
- 无新 opcode / 无新 IR 指令（纯元数据字段）。

## Pipeline Steps

- [ ] Lexer — 无（virtual/abstract/override 关键字已存在）
- [ ] Parser / AST — 无（`MethodDecl.Mods` 已承载）
- [ ] TypeChecker — 无
- [x] IR Codegen — `IrFunction.MethodFlags` + IrGen `_methodFlags(mods)`；ZbcWriter/ZbcReader/ZpkgReader
- [x] VM interp — `FuncSig`/`Function.method_flags` + reflection `build_method_info`
