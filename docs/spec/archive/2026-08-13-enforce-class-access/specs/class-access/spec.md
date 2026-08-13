# Spec: 类级访问强制（Class-level access control）

## ADDED Requirements

### Requirement: private 嵌套类仅外层类内可引用

`private` 嵌套类 `Outer+Inner`（默认或显式 `private`）只能在 `Outer` 的文本内（含 `Outer` 更深层嵌套）被
引用。判据：`env.CurrentClass()` 等于 `Outer`，或以 `Outer+` 为前缀。

#### Scenario: 外层类内引用私有嵌套类
- **WHEN** 在 `Outer` 的方法体内 `new Outer.Inner()` 或 `Inner x = ...`（`Inner` 默认 private）
- **THEN** 允许，无诊断

#### Scenario: 类外引用私有嵌套类
- **WHEN** 在 `Outer` 之外（自由函数或别的类）`new Outer.Inner()`
- **THEN** emit `E0404 AccessViolation`，消息形如 `cannot access private class \`Outer.Inner\``

#### Scenario: public 嵌套类类外引用
- **WHEN** 嵌套类显式 `public class Inner`，在 `Outer` 之外 `new Outer.Inner()`
- **THEN** 允许，无诊断

### Requirement: protected 嵌套类外层类及派生类可引用

`protected` 嵌套类 `Outer+Inner` 可在 `Outer` 及派生自 `Outer` 的类上下文中引用（`CurrentClass()` 沿基链
上溯能到 `Outer`）。

#### Scenario: 派生类引用基类的 protected 嵌套类
- **WHEN** `class Sub : Outer { ... new Outer.Inner() ... }`，`Inner` 为 `protected`
- **THEN** 允许，无诊断

#### Scenario: 无关类引用 protected 嵌套类
- **WHEN** 在与 `Outer` 无继承关系的类 / 自由函数中引用 `protected` 的 `Outer.Inner`
- **THEN** emit `E0404 AccessViolation`

### Requirement: internal 类同包引用放行（跨包强制为 follow-up）

无修饰符顶层类默认 `internal`。本 change **同包**引用一律放行（`IsImported==false`）。**跨包 `internal` 类
强制**（`IsImported && Visibility=="internal"` → deny）需类可见性进 zbc/zpkg 元数据（格式 bump），拆为
follow-up change `enforce-crosspkg-internal-class`——`CheckTypeRef` 已含该分支，但本 change 不序列化类可见性，
imported 类 `Visibility` 默认 `public`，故该分支对 imported 暂不触发。

#### Scenario: 同包引用 internal 类
- **WHEN** 同一包内引用一个 `internal`（或无修饰符默认）顶层类
- **THEN** 允许，无诊断

#### Scenario: 跨包引用 public 类
- **WHEN** 包 B 引用包 A 的 `public class Logger`
- **THEN** 允许，无诊断（stdlib 全量跨包 public 引用即此回归门）

#### Scenario（follow-up，本 change 不覆盖）: 跨包引用 internal 类
- **WHEN** 包 B 引用包 A 声明的 `internal`（或无修饰符）顶层类 `Secret`
- **THEN**（follow-up 后）emit `E0404`，消息含 `from another package`；**本 change 暂放行**（类可见性未序列化）

### Requirement: 校验覆盖表达式/语句体的类型引用点

绑定期解析类型引用后即校验，覆盖以下引用形态；违规 emit `E0404` 但**不阻断绑定**（返回原解析结果，
单次编译可收集多条诊断）。

#### Scenario: 各引用点均被校验
- **WHEN** 引用不可访问类型出现在 `new T()` / 局部 `T x` / `(T)e` / `e is T` / `e as T` / `typeof(T)` /
  `T.staticMember` / `catch (T e)` / 泛型实参 `List<T>`
- **THEN** 每处独立 emit 一条 `E0404`

#### Scenario: 合法程序绑定/字节不变
- **WHEN** 编译一个不含越界类型引用的程序（如 z42c 自身、stdlib）
- **THEN** 无新增诊断；Bound 树 / IR 逻辑不变；z42c 自举 `gen1 == gen2` 逐字节保持

### Requirement: 校验覆盖声明签名位置的类型引用（全覆盖）

除表达式/语句体外，声明签名位置命名一个不可访问类型同样违规——收集期解析后校验，emit 到收集期 diag bag。

#### Scenario: 字段 / 参数 / 返回类型引用不可访问（private 嵌套）类型
- **WHEN** 在类 C 中声明 `Outer.Inner f;`（`Inner` 为 `Outer` 的 private 嵌套类、C≠Outer）、或
  `void g(Outer.Inner x)`、或 `Outer.Inner h() { ... }`
- **THEN** 每处 emit `E0404 AccessViolation`

#### Scenario: 基类引用不可访问（private 嵌套）类型
- **WHEN** `class X : Outer.Inner`，其中 `Inner` 为 `Outer` 的 private 嵌套类
- **THEN** emit `E0404 AccessViolation`

#### Scenario: 基名无法解析成类则跳过
- **WHEN** 基/接口名是泛型形参、未知名或歧义
- **THEN** 不校验、不误报（保守放行）

## 无格式变更（本 change）

本 change 纯诊断层——`Z42ClassType.Visibility` 是**内存态、不序列化**；不改 zbc/zpkg 格式、不 bump 版本、
不动 codegen/IR。合法程序 Bound 树 / IR 字节不变，z42c 自举 `gen1==gen2` 逐字节保持。**跨包 internal 类**
所需的类可见性序列化（zbc 1.33 / zpkg 0.38 + Rust reader）在 follow-up `enforce-crosspkg-internal-class`。

## Pipeline Steps

受影响的 pipeline 阶段：
- [ ] Lexer —— 无
- [ ] Parser / AST —— 无（访问修饰符已解析进 `Mods`）
- [x] TypeChecker —— 类可见性收集（`SymbolCollector._putClassStub`）+ 引用点强制（`AccessChecker.CheckTypeRef`，绑定期体引用 + 收集期声明签名）
- [ ] IR Codegen —— 无（无格式变更）
- [ ] VM interp —— 无
