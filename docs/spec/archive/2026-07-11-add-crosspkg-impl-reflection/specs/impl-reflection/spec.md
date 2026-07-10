# Spec: 跨包 impl 反射

## ADDED Requirements

### Requirement: VM 读 IMPL 段建注册表

#### Scenario: 加载含 impl 的 zpkg
- **WHEN** VM 加载一个 IMPL 段含 `{target: A.Thing, trait: C.Greeter}` 的 zpkg
- **THEN** impls 注册表含 `A.Thing → [C.Greeter]`

#### Scenario: 无 IMPL 段 / 空 IMPL
- **WHEN** VM 加载 .zbc 或 IMPL 为空的 zpkg
- **THEN** 注册表不变、加载不报错

### Requirement: GetInterfaces 含跨包 impl trait

#### Scenario: 直接 impl
- **WHEN** 包 B `impl Greeter for Thing`（Thing 在包 A）且两包已加载，反射 `typeof(Thing).GetInterfaces()`
- **THEN** 结果含 Greeter（FQ 身份）

#### Scenario: 声明接口不回归
- **WHEN** Thing 自身声明 `: IBase`
- **THEN** GetInterfaces 同时含 IBase 与 impl 加的 Greeter（去重、传递闭包语义不变）

#### Scenario: 未加载包的 impl 不可见
- **WHEN** 声明了 impl 的包 B 未被加载
- **THEN** GetInterfaces 不含该 trait（与「其 impl 方法不可调」一致）

## IR Mapping
无新 opcode / 无格式变化（读现有 zpkg IMPL 段）。

## Pipeline Steps
- [x] VM interp — IMPL 解析 + 注册表 + reflection builtin（其余 pipeline 无涉）
