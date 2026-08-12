# Spec: Access Control Enforcement

## ADDED Requirements

### Requirement: private 成员仅声明类内可访问

#### Scenario: 类外访问 private 字段 → E0404
- **WHEN** 在声明类 `A` 之外（自由函数 / 其它类）对 `A` 的实例读或写 `private` 字段
- **THEN** emit `E0404`，消息形如 `cannot access private field 'a' of 'A'`

#### Scenario: 同类内访问 private 字段（含其它实例）→ 通过
- **WHEN** 在 `A` 的方法体内访问 `this` 或另一 `A` 实例的 `private` 字段
- **THEN** 无诊断（private 按类而非实例判定，镜像 C#）

#### Scenario: 派生类访问基类 private → E0404
- **WHEN** `class D : B`，在 `D` 的方法体内经 `B` 实例访问 `B` 的 `private` 成员
- **THEN** emit `E0404`（private 不被继承开放）

#### Scenario: 类外调用 private 方法 → E0404
- **WHEN** 在声明类之外调用某实例的 `private` 方法
- **THEN** emit `E0404`

### Requirement: protected 成员限声明类与派生类

#### Scenario: 派生类访问基类 protected → 通过
- **WHEN** `class D : B`，在 `D` 方法体内访问 `B` 的 `protected` 成员（经 `this` 或 `D`/`B` 实例）
- **THEN** 无诊断

#### Scenario: 无关类访问 protected → E0404
- **WHEN** 在与声明类无继承关系的上下文访问 `protected` 成员
- **THEN** emit `E0404`

#### Scenario: 跨包派生类访问基类 protected → 通过
- **WHEN** 本地 `D` 继承自 imported 包的 `B`，在 `D` 内访问 `B` 的 `protected` 成员
- **THEN** 无诊断（protected 跨包对派生开放）

### Requirement: internal 成员限同包

#### Scenario: 同包访问 internal（无修饰符）成员 → 通过
- **WHEN** 在当前编译包内访问同包某类的无修饰符（默认 internal）或显式 `internal` 成员
- **THEN** 无诊断

#### Scenario: 跨包访问 internal 成员 → E0404
- **WHEN** 访问 imported 包中某类的 `internal`（含无修饰符默认）成员
- **THEN** emit `E0404`，消息形如 `cannot access internal member 'x' of 'B' from another package`

### Requirement: override 继承基类可见性

#### Scenario: 无修饰符 override 跨包可调用
- **WHEN** 跨包调用某类 `override string ToString()`（无显式修饰符）
- **THEN** 无诊断（override 继承基类 public 可见性）

### Requirement: record 定位字段公有

#### Scenario: 跨包读 record 定位字段
- **WHEN** 跨包读 `record R(string A, ...)` 的定位字段 `A`
- **THEN** 无诊断（record 定位字段合成为 public）

### Requirement: public 成员不受限

#### Scenario: 任意上下文访问 public 成员 → 通过
- **WHEN** 在任意位置（跨包、无关类）访问 `public` 字段 / 方法 / 属性
- **THEN** 无诊断

### Requirement: 覆盖全部成员访问形态

#### Scenario: 属性 getter / setter 遵循可见性
- **WHEN** 类外读 `obj.P`（getter）或写 `obj.P = v`（setter），而属性为 `private`/`protected`/跨包 `internal`
- **THEN** emit `E0404`

#### Scenario: 静态成员访问遵循可见性
- **WHEN** 类外经 `ClassName.field` / `ClassName.Method()` 访问 `private`/`protected`/跨包 `internal` 静态成员
- **THEN** emit `E0404`

### Requirement: 校验不改变合法程序的产物

#### Scenario: z42c 自举字节不动点保持
- **WHEN** 强制层落地后重跑自举
- **THEN** gen1 与 gen2 产物 byte-identical（校验为纯诊断层，不改 codegen）；stdlib / e2e / cross-zpkg / compiler 全绿

## IR Mapping

无新增 IR 指令 / opcode。成员可见性 int 加值 `3=internal`（既有 `u8` 字段扩值域，**不改字节布局、无格式
bump**）；`_visCode` 无修饰符→3、override→0；`_visStr` 3→"internal"。合法程序的 Bound 树与 IR 结构不变
（无修饰符成员 vis 字节 0→3 是唯一 metadata delta，被自举 gen1==gen2 同步吸收）。

## Pipeline Steps

受影响阶段：

- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [x] TypeChecker（Bound 绑定期）— 新增 AccessChecker，在成员解析 5+ 处接入
- [ ] IR Codegen — 无
- [ ] VM interp — 无
