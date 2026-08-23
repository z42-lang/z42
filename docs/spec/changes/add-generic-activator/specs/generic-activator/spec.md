# Spec: 泛型 Activator.CreateInstance<T>

## ADDED Requirements

### Requirement: 泛型无参构造 `Activator.CreateInstance<T>()`

#### Scenario: 用户类无参构造
- **WHEN** 调用 `Activator.CreateInstance<Point>()`（`Point` 有无参 ctor 或无显式 ctor）
- **THEN** 返回一个 `Point` 实例，类型为 `Point`（`x as Point != null`），字段为各自默认值 / ctor 结果

#### Scenario: ctor 副作用生效
- **WHEN** `T` 的无参 ctor 有字段初始化 / 副作用
- **THEN** 返回实例已执行该 ctor（与 `new T()` 行为一致；与非泛型 `CreateInstance(typeof(T))` 同源）

#### Scenario: 跨包调用（typeof(T) 短名解析）
- **WHEN** 用户代码（entry module）调用 stdlib 的 `Activator.CreateInstance<UserType>()`
- **THEN** `typeof(T)` 解析到 `UserType` 的运行期 handle → 正确构造（依赖 `make_type_from_name` 无点短名兜底，
  add-json-serde 已落地）；目标类型在用户程序内唯一时稳解析

#### Scenario: 泛型方法内转发方法级形参 T
- **WHEN** 另一泛型方法 `Foo<T>()` 内调用 `Activator.CreateInstance<T>()`（T 为 Foo 的**方法级形参**）
- **THEN** 调用点发**转发标记 `$mta:<idx>`**（idx = T 在 Foo 方法级形参表的下标），运行期在拷入被调方
  frame 前按**调用方**（Foo）frame.method_type_args[idx] 解析成具体名 → 被调方 `typeof(T)` 拿到具体
  handle → 正确构造。（此前发字面 "T" → 落空丢 handle，是 #240 通用缺口。）

### Requirement: 方法级泛型形参转发到嵌套泛型调用（#240 缺口修复）

#### Scenario: 顶层类型实参转发
- **WHEN** 泛型方法 `Foo<T>()` 内以 `T` 为**顶层**类型实参调另一泛型方法/构造（`Bar<T>()`）
- **THEN** 运行期被调方的方法级形参物化为 Foo 当前的 T 具体类型（而非字面形参名）

#### Scenario: 非转发实参不受影响（字节不变）
- **WHEN** 类型实参是具体类型（`Bar<int>` / `Bar<UserType>`）或类级形参
- **THEN** 仍发既有 FQ 名（无 `$mta` 标记）；无转发标记的调用产物字节不变

## MODIFIED Requirements

`CreateInstance(Type)`（非泛型）行为不变；本 change **新增**泛型重载。**无格式版本变更**——
`method_type_args` 仍是 string[]，仅新增 `$mta:<idx>` 标记字符串（运行期在拷入 callee frame 前解析）。

## IR Mapping
- `typeof(T)` → `MethodTypeArgInsn`（既有 #240）；构造 → `__activator_create` builtin（既有）。
- **方法级形参转发**：`CallGeneric`/`VCallGeneric` 的 `method_type_args` string 项，若类型实参是外层
  方法级形参，编译期填 `$mta:<idx>`（否则 FQ 名）。运行期 `exec_call`/`exec_vcall` 在设置 callee
  frame.method_type_args 前，把 `$mta:<idx>` 替换为**调用方** frame.method_type_args[idx]。
- **无新 IR opcode / zbc·zpkg 格式改动。**

## Pipeline Steps
- [ ] Lexer / Parser / AST —— 无
- [x] TypeChecker（binding）—— `_applyMethodTypeArgs` 计算 `MethodTypeArgFwd`（方法级形参 → 下标）
- [x] IR Codegen —— `_methodTypeArgNames` 据 fwd 发 `$mta:<idx>`
- [x] VM interp —— `exec_call`/`exec_vcall` 按调用方 frame 解析 `$mta:<idx>`
- [x] stdlib 源 —— 新增 `CreateInstance<T>()` 薄壳
- [x] 测试 —— reflection.z42 加 [Test]（含转发用例）
