# Spec: delegate 元数据

## ADDED Requirements

### Requirement: delegate 进 TYPE（delegate-as-class）

#### Scenario: 非泛型 delegate
- **WHEN** z42c 编译 `delegate int Adder(int a, int b);`
- **THEN** TYPE 含 FQ 条目、`class_flags` bit6 置位、无字段

#### Scenario: 泛型 delegate
- **WHEN** z42c 编译 `delegate R Func<T, R>(T arg);`
- **THEN** TYPE 条目 TypeParams=["T","R"]；Invoke SIGS 参数类型按名（"T"）引用

#### Scenario: 合成 Invoke
- **WHEN** delegate 编译后
- **THEN** SIGS 含 `<FQ>.Invoke`（实例、virtual、参数源拼写类型+名+P1-d 元数据、ret 同声明）

### Requirement: delegate 反射

#### Scenario: IsDelegate
- **WHEN** `Type.GetType("<fq delegate>")` 取句柄
- **THEN** `IsDelegate == true`；普通类/enum/接口 `IsDelegate == false`

#### Scenario: Invoke 签名反射
- **WHEN** 对 delegate Type `GetMethods()` 找 "Invoke"
- **THEN** MethodInfo：ReturnType 同声明、IsVirtual=true、GetParameters() 名/类型/元数据同声明

## IR Mapping
- TYPE `class_flags` bit6=delegate（无额外 payload）；每 delegate 一条 TYPE 记录 + 一条
  `<FQ>.Invoke` SIGS/FUNC 死体桩。无新 opcode。

## Pipeline Steps
- [x] IR Codegen — IrGen DelegateDecl pass + Invoke 桩；格式 bump 26/30
- [x] VM interp — CLASS_FLAG_DELEGATE + is_delegate + `__type_is_delegate`
