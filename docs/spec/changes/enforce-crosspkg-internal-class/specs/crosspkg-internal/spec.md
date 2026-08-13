# Spec: 跨包 internal 类引用强制（Cross-package internal class access）

## MODIFIED Requirements

### Requirement: internal 类跨包引用被拒

无修饰符顶层类默认 `internal`（含显式 `internal`）。**同包**引用一律放行；**跨包**引用一个声明为
`internal` 的类（`IsImported && Visibility=="internal"`）emit `E0404 AccessViolation`，消息形如
`cannot access internal class \`Secret\` from another package`。

> 承接 `enforce-class-access` ① 的同名 Requirement（当时同包放行 / 跨包 Deferred）。本 change 补齐类可见性
> 序列化，激活跨包 deny。

#### Scenario: 跨包引用 internal 类

- **WHEN** 包 A 声明 `internal class Secret { }`（或无修饰符 `class Secret { }`），包 B `new Secret()`
- **THEN** emit `E0404 AccessViolation`，消息含 `from another package`

#### Scenario: 同包引用 internal 类

- **WHEN** 同一包内引用本包 `internal` 类（`IsImported==false`）
- **THEN** 允许，无诊断

#### Scenario: 跨包引用 public 类

- **WHEN** 包 A 声明 `public class Api { }`，包 B `new Api()`
- **THEN** 允许，无诊断（可见性字节 = 0，`CheckTypeRef` 提前放行）

## ADDED Requirements

### Requirement: 类声明可见性序列化进 zbc/zpkg 元数据

zbc TYPE 记录在 `class_flags`（u8）之后紧随一个**类可见性字节**（u8：0=public/1=private/2=protected/
3=internal）。zpkg 内嵌 zbc，随之 bump。writer/reader 严格对称。

- zbc 格式版本 1.32 → **1.33**；zpkg 格式版本 0.37 → **0.38**（strict-pin，VM 精确匹配）。
- 载体为**独立字节**（非塞 `class_flags`——已满 u8；非仅 TSIG——import 从 zbc `cd` 重建）。

#### Scenario: public 类的可见性字节

- **WHEN** 序列化一个 `public` 类的 TYPE 记录
- **THEN** `class_flags` 后写入字节 `0`；reader 读回 `Visibility==0`

#### Scenario: internal 类的可见性字节 round-trip

- **WHEN** 序列化一个 `internal` 类，再由 importer 读回并经 `TsigReconcile` 还原
- **THEN** `ExportedClassZ.Visibility == "internal"`，`ImportedSymbolLoader` 填 `Z42ClassType.Visibility=="internal"`

### Requirement: VM 读类可见性字节但不上反射

Rust VM 的 zbc reader 必须读这个新字节以保持 TYPE 记录后续字段偏移正确，但 v1 **不**接入反射面
（`Type.IsPublic` 等类级可见性反射列 Deferred）。

#### Scenario: VM 读弃可见性字节

- **WHEN** VM 解析含可见性字节的 TYPE 记录
- **THEN** 读取该字节（read-and-discard）后正确解析后续 type-param / 字段 / 内联 struct 块，无偏移错位
