# Tasks: enum 成为独立类型

> 状态：🔴 DRAFT 待 User 确认 | 创建：2026-09-07
>
> **来源**：User 在 `add-argument-type-check` 的 Q2 裁决中选了「C# 语义」。因其为**语言语义变更**、
> 半径远超实参检查，按 [workflow.md](../../../../.claude/rules/workflow.md) Spec-First 拆为本独立 lang change。
>
> **依赖**：`add-argument-type-check` 先合（它已把 enum 位跳过并留了指向本变更的注释）。
> 本变更**必须摘掉**那个跳过（阶段 3.1），否则 enum 位永远不检查。
>
> 🔴 **未获 6.5 确认前不得写实现代码。**

## 进度概览
- [ ] 阶段 0: 摸清未知（codegen / 装箱 / 跨包 enum）——**先量再动手**
- [ ] 阶段 1: enum 类型表示（`IsEnum` 标记）
- [ ] 阶段 2: 转换与运算符
- [ ] 阶段 3: 摘跳过 + 调用点/测试改写
- [ ] 阶段 4: 验证与归档

## 阶段 0: 摸清未知（不写实现，只测）
- [ ] 0.1 实测 `PrimModel.IsScalarValue` / `StructLayout` / `BoxIfNeeded` 今天怎么看待 enum 的
      `Z42ClassType`（design D4 标为最大未知）
- [ ] 0.2 实测 `ImportedSymbolLoader` 今天怎么还原**跨包** enum 类型名（Q3）——若也退化成普通
      `Z42ClassType`，属 R1/R3/R5 同族的 imported 保真度缺口，须一并补
- [ ] 0.3 实测 `Color.Red.GetType()` 折叠今天走哪条路径（`EnumTypeName`）
- [ ] 0.4 把 0.1–0.3 结论写回 design.md D4 —— **不得先写实现再补结论**

## 阶段 1: enum 类型表示
- [ ] 1.1 `Z42ClassType` 加 `IsEnum` + `EnumUnderlying`（design D1 选项 B）
- [ ] 1.2 `SymbolTable.z42:255` enum 名解析时置标记
- [ ] 1.3 `MemberResolver.z42:36-46` `E.Member` 的 `BoundLitInt` 类型改为 enum 类型
      （`EnumTypeName` 字段保留，`GetType()` 折叠沿用）
- [ ] 1.4 `ImportedSymbolLoader` 跨包 enum 还原带标记（按 0.2 结论）

## 阶段 2: 转换与运算符
- [ ] 2.1 `Conversion` 新增 `ConvKind.ExplicitEnum`：**不进** `ImplicitOk`、**进** `Exists()`
- [ ] 2.2 `Conversion._classifyBuiltin` 加 enum ↔ 底层整数分支（双向）
- [ ] 2.3 `TypeOpTyper` 的 cast 路径放行 `(long)c` / `(Color)n`
- [ ] 2.4 `BinaryTypeTable`：`enum == enum` / `!=` / 关系比较（`<` `<=` `>` `>=`）
- [ ] 2.5 `PatternBinder`：enum 模式 + 关系模式（`>= HttpStatus.BadRequest`）
- [ ] 2.6 `ExprEmitter`：enum 静态类型仍发 i64；装箱按 i64（按 0.1 结论）

## 阶段 3: 摘跳过 + 调用点/测试改写
- [ ] 3.1 🔴 **摘掉** `OverloadBinder._checkOneArg` 里 `add-argument-type-check` 留的 `_isEnumSide` 跳过
      + 删对应注释（否则 enum 位永不检查 = 留一个假保障）
- [ ] 3.2 `src/tests/types/enum.z42:35,36,43,44` 按新语义改写
      （`Assert.Equal(404, (long)Status.NotFound)` / `(long)Direction.North == 0`）——
      **这 4 条成文断言的正是旧模型，必须改，不得绕**
- [ ] 3.3 `examples/patterns.z42:65` 关系模式确认可用
- [ ] 3.4 `z42.core/src/GC/GCHandle.z42` 按新语义确认（预期**无需改动**——两侧都是 `GCHandleType`）
- [ ] 3.5 全仓扫其余 79 处 enum 成员引用，逐处确认新语义下正确

## 阶段 4: 验证与归档
- [ ] 4.1 新增负例/正例测试（spec 场景逐条），🔴 用 `DumpBody`/`collectDiags` 断言，
      **不得只用 `SemanticDump.FirstErrorCode`**（不合并 collector 诊断 → 空门）
- [ ] 4.2 🔴 **反向自检**：整体退回改动，新增负例必须全红
- [ ] 4.3 完整 `xtask test` 全绿
- [ ] 4.4 🔴 **自举字节不动点**：gen1 == gen2 —— 实证 design D1 的「签名键不变」，不得靠推理
- [ ] 4.5 `xtask test stdlib --mode jit`（本地 `xtask test` 只跑 interp；enum 表示相关须验 JIT 面）
- [ ] 4.6 `xtask test bootstrap`（无新语法/无格式改动，上一 nightly 应照常编过）
- [ ] 4.7 文档：**改写** `docs/book/src/runtime/struct-value-semantics.md:232` 的 enum-as-int SoT 段；
      新建 `docs/book/src/language/enums.md` 并挂进 `SUMMARY.md`
- [ ] 4.8 归档 + 随 PR 一起提交

## 备注

- **本变更会让 `Color c = Color.Blue;` 第一次能编过**——那是今天就编不过的既存 bug（欠债表 bug D），
  不是新功能。
- 🔴 **禁止**用「在调用点补 cast」把红变绿来替代本变更（那是拿 cast 掩盖编译器 bug）。
