# Proposal: 补全泛型 `where` 约束校验

> 类型：lang（完整流程：DRAFT → User 确认 → IMPL → GREEN → COMMIT）
> 创建：2026-09-05

## Why

`where T : IFoo` 今天**写了等于没写**——不报错、也不校验。这比「不支持」更糟：不支持会报错，
假实现让使用者以为拿到了类型保护。

这不是一处遗漏，是**同一根因造成的三层塌陷**：

### 第 1 层 — 编译期只校验 1/4 的约束

[`ConstraintChecker._fillBundle`](../../../../src/compiler/z42c.semantics/src/ConstraintChecker.z42)
只认 base-class / `class` / `struct` / 型参引用四种，其余**静默延后**，注释原文：

```
// 泛型 base / interface / enum / 未知 → 延后（不报 false positive）
```

[`ConstraintBundle`](../../../../src/compiler/z42c.semantics/src/GenericConstraint.z42) 结构上
就只有 4 个位，**没有接口列表字段**。

更前置的一刀：`_fillBundle` 的条件是 `nt.ArgCount == 0 && symbols.HasClass(nt.Name)`。全仓 21 条
真实接口约束里**绝大多数是泛型接口**（`IEquatable<T>` / `IComparable<T>` / `INumber<T>`），
`ArgCount > 0` → **在接口判定之前就被整条丢弃**。这才是它们全部静默的直接原因。

### 第 2 层 — zbc writer 只写 1 个 flag 位，把运行期校验饿成了死代码

运行期 [`validate_type_arg_constraint`](../../../../src/runtime/src/corelib/reflection/generics.rs)
是一份**完整**实现——class / struct / enum / base / interfaces / `new()` / 型参引用七项俱全，
注释还写着：

> `Reflection (MakeGenericType) is the sole entry that bypasses the compiler's compile-time
> constraint checking, so it MUST self-police`

zbc reader 也认全部 flag 位（`type_reader.rs`：`0x01` class / `0x02` struct / `0x04` base /
`0x08` tpRef / `0x10` new() / `0x20` enum / `0x40` funcSig）。

**但 [`ZbcWriter.z42`](../../../../src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42) 只写
bit3 + 接口名列表**，注释直言「其余 z42c 暂仅接口约束」。于是运行期那五个分支**永远不触发**——
它自称的 self-police 也是空的。`MakeGenericType(typeof(int))` 那条测试能过，只因接口名列表确实写了。

### 第 3 层 — 没有任何门能发现前两层

负例**语料**曾在 `src/tests/errors/`，2026-05-12 搬进 `src/compiler/z42.Tests/Fixtures/`（C# 测试项目）。
**C# 编译器 2026-06-26 被移除时，整个 `z42.Tests/` 连同负例语料一起消失。** 今天 `src/compiler/`
下只剩 `z42c.driver` / `z42c.pipeline` / `z42c.semantics`；
[`src/tests/README.md:73`](../../../../src/tests/README.md) 仍指着那个已删目录——**文档比实现活得久**，
本身就是这层塌陷的旁证。

> **⚠️ 校正（2026-09-05 IMPL 期）**：DRAFT 初稿在此处断言「今天仍无 expected-compile-error
> 机制」，**这是错的**。语义层单文件诊断的机制一直在，且一直在 GREEN gate 内：
> [`SemanticDump`](../../../../src/compiler/z42c.semantics/src/SemanticDump.z42) 的
> `FirstErrorCode` / `FirstErrorMessage` / `FirstErrorPos` / `ErrorCount` 接一段源码串回诊断，
> 由 `xtask test compiler`（stage 5）的 `[Test]` units 驱动；活例见
> [`undefined_type_tests.z42`](../../../../src/compiler/z42c.semantics/tests/typecheck/undefined_type/undefined_type_tests.z42)。
> 且 `ConstraintChecker` 与 `TypeChecker` **共用同一个 `DiagnosticBag`**
> （`TypeChecker.z42:41` 构造、`:305` 驱动 `Resolve`，校验落在 `ConstructTyper.z42:156` /
> `MemberResolver.z42:364`），故 where 约束的诊断天然对 `SemanticDump` 可见。
>
> ⇒ **真正缺的只是「多文件 / 跨包」负例门**（E0404 cross-zpkg internal 那一类），
> 而 where 约束全是单文件语义诊断，用不上它。PR-0 因此从「造机制」缩为「用机制」，见 What。

真正没有门的是**跨包**负例——
[`class_internal_access/README.md`](../../../../src/tests/cross-zpkg/class_internal_access/README.md)
自己写着：

> cross-zpkg 自动 runner **无 expected-compile-error 模式** …… 故本目录**故意不含**
> `expected_output.txt`，runner 会跳过，不影响 GREEN。

### 根因一句话

**自举迁移只搬了「能编过」的正例；「应该报错」的负例随 C# 测试项目一起没了，于是校验退化成静默、
无人知晓。** 这是 memory `format-fixture-antirot-gates`「没有测试盯着的约定迟早会烂」的第二次发生。

同一模式的同期受害者还有**命名实参**（语义层 `OverloadBinder._adaptArgs` 齐备，但
`ExprParser._parseCall` 无 Colon 分支；归档说已 ship，编译器里没有）——本变更不含它，单独立项。

## What

按运行期已有的七项判定为准，把编译期补齐到同一套规则；并**先给测试体系补上能承载负例的门**，
否则修完还会烂第三次。

本轮范围（同包 + 零格式风险）：

| 阶段 | 内容 |
|---|---|
| ~~**PR-0**~~ | ~~测试体系加 `expected_error.txt` sidecar~~ **已撤销**：机制已存在（见上「校正」），负例改为跟着 `z42c.semantics` 走的一个平铺 `[Test]` 文件，并进 PR-1 |
| **PR-A** | 修 `ZbcReader` 漏读 bit2（**真 bug**，见 design §3） |
| **PR-1** | 同包接口约束真校验（去 `ArgCount` 过滤 + Bundle 加接口字段 + 合并两份重复 `_fillBundle` + 接口继承闭包）+ 负例单测 |
| **PR-2** | `enum` 约束（含 parser）+ `new()` 约束 |
| **PR-4** | 诊断质量：用已有的 `WhereClause.Span` 替掉 `_noSpan()`；未知约束名从静默改报 E0443 |

**本轮不做**（下一轮，理由见 design §6）：

- **PR-3** ZbcWriter 置 bit0/1/2/4/5，接活运行期死分支——收益独立，依赖 PR-A
- **PR-5** 跨包约束持久化——踩 bootstrap-seed 第二根轴（stdlib API 面），需卡 nightly 节奏
- **跨包 / 多文件负例门**（E0404 cross-zpkg internal 那类今天仍靠手工 fixture）——与 where
  约束正交，登记 Deferred 单独立项

## 非目标

- **不发明新语义**：运行期 `validate_type_arg_constraint` 是既成 SoT，编译期照抄。不接受两边各判各的。
- **不做格式 bump**：flag 位与接口名列表在 zbc 里早已规约，z42c 只是没写。
- **不改 `where` 语法表达力**：不引入 `+` 组合、关联类型等 `generics.md` 里的 z42 扩展。

## User 已裁决

1. **PR-1 先以 warning 落地探一轮**，跑全仓 + stdlib 拉出新增诊断清单对账，确认零误报后再翻 error。
2. `src/tests/types/struct_generic_container.z42` 里的 `struct P` / `struct Tagged`
   **补 `: IEquatable<>`**（而非放宽 `Dictionary` 的 `TKey` 约束）。
3. **（2026-09-05 IMPL 期）PR-0 撤销**：既有 `SemanticDump` + `[Test]` 机制已够用且已在 GREEN
   gate 内，不另造第二套。负例**跟着类库走、尽量扁平单一职责**——落
   `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42`，与 `typecheck_tests.z42`
   平级并列，**不**放 `src/tests/`、**不**再嵌一层子目录。
