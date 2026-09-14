# add-iface-visibility-check — 接口成员必须 `public` 实现（E0412 A1）

## 背景

`fix-iface-satisfaction-gaps`（#636）把接口满足性校验补齐了**种类**（static/instance）与
**返回类型**，但仍漏一维：**可见性**。`InheritanceResolver._checkOneIfaceMethod` 在 `MangleKey`
（名 + 形参）命中后从不比 `Visibility` ⇒ 非 public 实现静默满足接口契约。

接口成员是隐式 public 契约：`private`/`internal`/`protected` 实现**无法经接口静态类型调用**，
等同没实现（对齐 C# CS0535/CS0737）。此前 `class C : I { private int M(){…} }` 静默通过 = 真洞。

这是 [`add-associated-types-program`](../../../../.claude/…) gap 扫描 Tier A 的 A1 项。

## 变更

在 `_checkOneIfaceMethod` 的 static 检查之后、返回类型检查之前补一道：命中 MangleKey 后若
`cm.Visibility != "public"` → 发 E0412 `InterfaceMismatch`。

- **口径：严格（User 裁决 2026-09-14）**。类成员**无修饰默认 `private`**（`SymbolCollector:385`
  的 `containing != ""` → "private"，C# 惯例），故 `class C : I { int M(){…} }` 同样被拦——
  接口实现必须**显式写 `public`**。与全仓惯例一致（stdlib/compiler 接口实现一律显式 `public`）。
- **只覆本包接口**：跨包接口在既有 `it.IsImported` 早退处已跳过（导入侧 `Visibility` 与
  `IsStatic` 同不可靠，端到端修需动导出侧 + 可能格式 bump，超范围）。

## 非目标

- 不改 MangleKey（派发键，改它撼动自举字节）。
- 不动跨包导入接口的可见性保真（另有 Deferred `imported-iface-static-member-fidelity`）。

## 影响面

- **零字节漂移**：纯诊断、不回灌发射；全仓无任何非-public 接口实现（爆炸半径实测 0）。
- **无格式 bump**：复用既有 E0412 家族、无新 IR/wire。
