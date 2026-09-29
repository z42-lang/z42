# Spec: 类型名与内建基元拼写冲突（E0499）

## ADDED Requirements

### Requirement: 类型名不得与内建基元的 PascalCase 拼写相同

在 `Std` 之外声明的 **顶层** `class` 或 `struct`，其名字 **不得** 是下列 14 个内建基元
PascalCase 拼写之一：

```
Byte  SByte  Int16  Int32  Int64  UInt16  UInt32  UInt64
Single  Double  Boolean  Char  String  Object
```

违反即 **E0499**（Error）。

判据的规范定义：**`PrimModel.Canon(<声明的简单名>)` 的结果是一个内建基元关键字**
（等价于 `PrimModel.IsScalarValue` 为真，或结果为 `string` / `object`）。
以 `Canon` 为准而不是以上面那张表为准 —— 表是 `Canon` 的**当前**值域，
二者若分叉，`Canon` 是唯一真相源。

#### Scenario: 顶层 struct 用内建拼写命名

- **GIVEN** 一个 CU，其 `namespace` 不是 `Std`
- **WHEN** 它声明 `struct Single { public int X; }`
- **THEN** 编译失败，诊断码 `E0499`
- **AND** 消息指出该名字是内建基元 `float` 的拼写，并要求改名

#### Scenario: 顶层 class 用内建拼写命名

- **GIVEN** 同上
- **WHEN** 它声明 `class String { public int X; }`
- **THEN** 编译失败，诊断码 `E0499`

#### Scenario: `Std` 里的声明不受限

- **GIVEN** 一个 CU，其 `namespace` 是 `Std`
- **WHEN** 它声明 `struct Single { … }`
- **THEN** 本规则**不报** E0499

> 这条豁免是 `z42.core` 能编过的原因：`Std.Single` **就是** `float` 的包装类型声明。
> 用户包若也往 `Std` 里声明这 14 个名字，由**既有的 E0606**（本包遮蔽导入）拦截 ——
> 本规则不重复管辖。

#### Scenario: enum / interface 不受限

- **WHEN** 声明 `enum Single { A, B }` 或 `interface Single { int F(); }`
- **THEN** 本规则**不报**

> 实测这两种形态不受 `Canon` 折叠影响、行为正确。为它们加约束等于凭空扩大语言限制。

#### Scenario: 嵌套类型不受限

- **WHEN** 声明 `class Outer { public struct Single { public int X; } }`
- **THEN** 本规则**不报**

> 嵌套类型的符号表键是 `Outer+Single`，`Canon` 不会把它折成基元。

#### Scenario: 关键字拼写由语法层拦截

- **WHEN** 声明 `struct float { … }`
- **THEN** 报 **E0202**（expected type name），不是 E0499

> 小写关键字是词法保留字，走不到语义层。本规则只管 PascalCase 拼写。

### Requirement: 诊断必须在**声明位**报告

E0499 的 span 指向**类型声明本身**，而不是使用点。

#### Scenario: 声明了但从不使用

- **GIVEN** 一个 CU 声明 `struct Int32 { public int X; }` 且**从不实例化它**
- **THEN** 仍然报 E0499

> 今天这种形态恰好不崩（崩点在字段访问），但它仍是一个被静默重定义的内建类型。
> 把诊断挂在声明位，规则才可陈述、才不依赖「有没有正好触发崩溃」。
