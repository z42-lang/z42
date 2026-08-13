# Spec: 类级访问控制补全（class-access-control）

## ADDED Requirements

### Requirement: 类可见性反射谓词（Type.IsPublic 族）

`Type` 暴露与 C# `System.Type` 对齐的可见性谓词，从 #184 已序列化的 TYPE 可见性字节读出。
顶层类型只可能 public/internal（③ 强制后），嵌套类型四级皆可能。无 TYPE handle 的类型
（基元 / 数组 / 未知）所有谓词返回 `false`（与既有 `IsSealed` 等谓词一致）。

映射（`vis` = 可见性字节 0=public/1=private/2=protected/3=internal；`nested` = FQ 名含 `+`）：

| 属性 | 语义 |
|------|------|
| `IsPublic` | `!nested && vis==0` |
| `IsNotPublic` | `!nested && vis!=0`（顶层非 public ⟺ internal） |
| `IsNestedPublic` | `nested && vis==0` |
| `IsNestedPrivate` | `nested && vis==1` |
| `IsNestedFamily` | `nested && vis==2`（protected） |
| `IsNestedAssembly` | `nested && vis==3`（internal） |

#### Scenario: 顶层 public 类反射
- **WHEN** `typeof(PublicApi).IsPublic`（`public class PublicApi`）
- **THEN** 返回 `true`；`IsNotPublic` / 所有 `IsNested*` 返回 `false`

#### Scenario: 顶层默认（internal）类反射
- **WHEN** `typeof(Engine).IsNotPublic`（`class Engine`，无修饰符 → internal）
- **THEN** 返回 `true`；`IsPublic` 返回 `false`

#### Scenario: 嵌套四级反射
- **WHEN** 反射 `Outer` 内的 `private Node` / `protected P` / `internal S` / `public Iter`
- **THEN** 分别 `IsNestedPrivate` / `IsNestedFamily` / `IsNestedAssembly` / `IsNestedPublic` 为 `true`，其余为 `false`

#### Scenario: 基元/数组类型
- **WHEN** `typeof(int).IsPublic` 或 `typeof(int[]).IsNestedPrivate`
- **THEN** 返回 `false`（无 TYPE handle，不臆断）

## IR Mapping
无新增 opcode / 无格式 bump。复用 #184 的 zbc TYPE 可见性字节（zbc 1.33 / zpkg 0.38）。
VM 侧新增 `ClassDesc.visibility: u8` → `TypeDesc.visibility: u8` 字段承载，6 个 builtin
（`__type_is_public` 等）读之。

## Pipeline Steps
- [ ] VM 元数据 reader（`zbc_reader.rs`：discard → store）
- [ ] VM loader（`ClassDesc` → `TypeDesc`）
- [ ] VM corelib（reflection builtins + 注册）
- [ ] stdlib（`Type.z42` extern 属性）

---

### Requirement: 不一致可访问性诊断（E0441）

当一个成员 / 类型声明的签名或基类列表**暴露**了比该声明本身更低可见性的类型时，
emit `E0441`。可见性 rank：`public=3 > internal=2 > protected=1 > private=0`；
规则：**被暴露类型的 rank 必须 ≥ 暴露声明的 rank**，否则 E0441。

覆盖的暴露点（暴露声明 → 被暴露类型）：
- `public`/`internal`/`protected` 字段 → 字段类型
- 方法 → 返回类型 + 各参数类型（构造器同，为 `MethodDecl`）
- 属性 / 索引器 → 类型
- 类声明 → 基类 + 各接口

被暴露类型的可见性取其类级可见性（`Z42ClassType.Visibility` / `Z42InterfaceType.Visibility`）；
非类/接口类型（基元 / 泛型形参 / 未知 / func）rank 视为 `public`（不触发，绝不误报）。

#### Scenario: public 方法返回 internal 类
- **WHEN** `public class Api { public Secret Get() {...} }`，`Secret` 为 internal
- **THEN** emit `E0441`（"inconsistent accessibility: return type ..."）

#### Scenario: public 字段暴露 internal 类型
- **WHEN** `public class Api { public Secret s; }`
- **THEN** emit `E0441`

#### Scenario: public 类继承 internal 基类
- **WHEN** `public class Derived : InternalBase {}`
- **THEN** emit `E0441`

#### Scenario: internal 方法返回 private 嵌套类
- **WHEN** `class Api { internal Node Peek() {...} }`，`Node` 为 private 嵌套类
- **THEN** emit `E0441`

#### Scenario: 一致（不报）
- **WHEN** `public class Api { public string Name; internal Secret S; private Node N; }`（每个暴露 rank ≤ 类型 rank）
- **THEN** 无 E0441

#### Scenario: private 成员暴露任意（不报）
- **WHEN** `class Api { private Secret s; }`（private 成员 rank 0，最低，恒满足）
- **THEN** 无 E0441

## Pipeline Steps
- [ ] TypeChecker（`DeclBinder._bindClass` → `AccessChecker.CheckExposure`）

---

### Requirement: 顶层声明拒绝 private/protected（E0442）

顶层（非嵌套）class / struct / record / interface / enum / 函数不得标 `private` 或
`protected`（模块作用域下无意义）；违反 emit `E0442`，声明期（parser bag）。
`internal`（显式或默认）/ `public` 合法。

#### Scenario: 顶层 private 类
- **WHEN** `private class Foo {}`（顶层）
- **THEN** emit `E0442`（"top-level ... cannot be private or protected"）

#### Scenario: 顶层 protected 接口 / 枚举 / 函数
- **WHEN** `protected interface I {}` / `protected enum E {A}` / `protected void f() {}`（顶层）
- **THEN** emit `E0442`

#### Scenario: 顶层 internal / public（不报）
- **WHEN** `internal class A {}` / `public class B {}` / `class C {}`（默认 internal）
- **THEN** 无 E0442

#### Scenario: 嵌套 private/protected（不报，合法）
- **WHEN** `class Outer { private class Node {} protected int x; }`
- **THEN** 无 E0442（嵌套走 MemberParser 路径，不经本检查）

## Pipeline Steps
- [ ] Parser（`ParseCompilationUnit` 顶层分派点）

---

### Requirement: 接口类型可见性建模 + 跨包强制

接口与类对称携带可见性：默认顶层 internal / 嵌套 private，可显式 public / internal
（顶层）。`internal` 接口跨包引用 → `E0404 ... from another package`（复用 `CheckTypeRef`）。
无格式 bump（接口 TYPE record 已随 #184 携带可见性字节，此前恒写 0=public）。

#### Scenario: 跨包引用 internal 接口
- **WHEN** 包 A 的 `interface Handler {}`（默认 internal），包 B `Handler h;` 引用
- **THEN** emit `E0404 ... from another package`

#### Scenario: 跨包引用 public 接口（不报）
- **WHEN** 包 A `public interface Api {}`，包 B 引用
- **THEN** 无诊断

#### Scenario: 同包引用 internal 接口（不报）
- **WHEN** 同包内引用 internal 接口
- **THEN** 无诊断（`!IsImported` 放行）

#### Scenario: 接口可见性 round-trip
- **WHEN** `public interface Api {}` 编入 zpkg 再被依赖包导入
- **THEN** 导入侧 `Z42InterfaceType.Visibility == "public"`（经 TsigReconcile → ExportedInterfaceZ → ImportedSymbolLoader）

## IR Mapping
无新增 opcode / 无格式 bump。接口经 `_interfaceDesc` 走同一 `IrClassDesc` → 同一 TYPE
record，可见性字节位已由 #184 无条件写/读；本 change 让接口写入真实可见性（此前恒 0）。

## Pipeline Steps
- [ ] TypeChecker / 符号采集（`Z42InterfaceType.Visibility`、`_passInterfaces`、`CheckTypeRef` 接口分支）
- [ ] IR Codegen（`_interfaceDesc` 设可见性）
- [ ] zpkg 元数据（`ExportedInterfaceZ` / `TsigReconcile._rebuildInterface` / `ImportedSymbolLoader`）
