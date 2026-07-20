# Spec: 接口成员枚举

## ADDED Requirements

### Requirement: typeof(接口).GetMethods() 返回声明方法

#### Scenario: 接口声明方法可枚举
- **WHEN** `interface IShape { double Area(); void Scale(double f); }`，取 `typeof(IShape).GetMethods()`
- **THEN** 返回含 `Area` 与 `Scale` 的 `MethodInfo[]`（数量 ≥ 2；名字为源码名，去 mangle 后缀）

#### Scenario: MethodInfo 签名正确
- **WHEN** 检视 `Area` 的 MethodInfo
- **THEN** `ReturnType.Name == "double"`，`GetParameters().Length == 0`

#### Scenario: 带参方法的参数元数据
- **WHEN** 检视 `Scale` 的 MethodInfo
- **THEN** `GetParameters().Length == 1`，参数类型 `double`（参数名若有 debug 符号则为源名）

#### Scenario: 接口方法标记为 abstract + virtual
- **WHEN** 检视任一接口方法的 MethodInfo
- **THEN** `IsAbstract == true` 且 `IsVirtual == true`（接口方法隐式抽象虚方法）

### Requirement: 接口方法纳入 GetMembers()

#### Scenario: GetMembers 含接口方法
- **WHEN** `typeof(IShape).GetMembers()`
- **THEN** 结果含 `Area` / `Scale` 的 MethodInfo（method 切片，同 GetMethods）

### Requirement: 只返直接声明方法（不含继承接口）

#### Scenario: 派生接口不含基接口方法
- **WHEN** `interface IBar : IFoo { void Extra(); }`，`typeof(IBar).GetMethods()`
- **THEN** 含 `Extra`，**不含** IFoo 声明的方法（对齐 C# 默认；基接口方法经 `typeof(IBar).GetInterfaces()` 各自 `GetMethods()` 取）

### Requirement: 不回归既有反射与自举

#### Scenario: 类反射不受影响
- **WHEN** 既有类的 `GetMethods()` / `GetFields()` / `GetProperties()` 用例
- **THEN** 全部通过（接口方法发射是新增路径，不动类路径）

#### Scenario: 自举字节不动点
- **WHEN** z42c `--workspace` 自建两遍（gen1 / gen2）
- **THEN** 7 子包逐字节一致（发射确定性：接口方法按稳定序 emit）

## IR / Pipeline

受影响阶段：
- [ ] Lexer / Parser — 不涉及（接口方法声明已解析）
- [x] IR Codegen（IrGen 接口成员发射 + ClassDescBuilder 接口方法表）
- [x] 格式（TYPE 段方法块——大概率复用现有结构，见 proposal Q1）
- [x] VM 读取（若 Q1 判定复用，则 reader 不变；`builtin_type_methods` 天然消费）

## 格式映射

- 接口方法签名 → SIGS/FUNC（签名桩，同 abstract 方法）：`method_flags = abstract|virtual`。
- 接口方法引用 → TYPE 段该接口的 vtable/own_methods 块（qualified 名）。
- **无新 opcode**。version bump 视 Q1 判定（复用现块 → 无 bump；整块此前省略 → bump 1.27→1.28 / 0.32→0.33）。
