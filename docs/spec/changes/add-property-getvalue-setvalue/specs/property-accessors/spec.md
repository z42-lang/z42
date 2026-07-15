# Spec: PropertyInfo.GetValue / SetValue

## ADDED Requirements

### Requirement: 反射读属性值（GetValue）

#### Scenario: 读可读实例属性
- **WHEN** `PropertyInfo p = typeof(Point).GetProperties()[i]`（`p.CanRead == true`），`Point pt = new Point(3, 4)`
- **THEN** `p.GetValue(pt)` 返回该属性 getter 的结果（如 `3`），类型为 `object`（装箱值）

#### Scenario: 读继承属性
- **WHEN** 属性在基类声明、`target` 是派生类实例
- **THEN** GetValue 经基类 getter 限定名正确返回值

#### Scenario: 只写属性 GetValue
- **WHEN** `p.CanRead == false`（无 getter）
- **THEN** `p.GetValue(target)` 抛 `Std.Exception`（信息含属性名 + "no getter"）

### Requirement: 反射写属性值（SetValue）

#### Scenario: 写可写实例属性
- **WHEN** `p.CanWrite == true`，`p.SetValue(pt, 42)`
- **THEN** 调用 setter，`p.GetValue(pt)` 随后返回 `42`

#### Scenario: 只读属性 SetValue
- **WHEN** `p.CanWrite == false`（无 setter）
- **THEN** `p.SetValue(target, v)` 抛 `Std.Exception`（信息含属性名 + "no setter"）

### Requirement: 异常透明传播

#### Scenario: 访问器内部 throw 原类型传播
- **WHEN** getter/setter 体内 `throw new MyError(...)`
- **THEN** GetValue/SetValue 把**原类型** `MyError` 传出（可被调用方 `try/catch MyError` 捕获），与 `MethodInfo.Invoke` 一致

### Requirement: interp 与 jit 一致

#### Scenario: 两模式结果一致
- **WHEN** 同一 GetValue/SetValue 用例在 `--mode interp` 与 `--mode jit` 下运行
- **THEN** 结果与异常行为一致

## Pipeline Steps

受影响阶段（非编译期——运行期 builtin + stdlib）：
- [ ] Lexer / Parser / TypeChecker / IR Codegen — **不涉及**（无新语法、无格式变更）
- [x] VM interp（新 builtin `__property_get_value` / `__property_set_value`）
- [x] stdlib（PropertyInfo.z42 新方法）

## IR / 格式映射

**无**。属性值经运行期 `exec_function` 调既有访问器函数；`__getterQualified` / `__setterQualified` 是运行期对象槽，不入 zbc/zpkg。**无 version bump。**
