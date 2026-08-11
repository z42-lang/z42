# Proposal: 统一类型转换分类器（Conversion classifier）

> 三 PR 阶梯之 **PR1 / 3**。总纲：C# 风格隐式/显式转换体系（比 C# 更严更可预测）。
> PR1 立基础设施（本变更）→ PR2 收紧规则 + 迁移 → PR3 用户自定义 `implicit`/`explicit`。

## Why

当前类型转换的判定散落在一堆返回 `bool` 的谓词里——`TypeFactsTc._isAssignable`、
`Z42Type.IsAssignableTo`、cast 绑定、`BoxIfNeeded`——它们只回答"能不能转"，**不携带**
"这是哪一种转换 / 隐式还是显式 / 该调用哪个转换方法"。

PR2（把隐式窄化收紧成显式、有损浮点降级为显式、为所有数值转换插 `ConvertInstr`）和
PR3（用户自定义 `implicit`/`explicit operator`）都需要这条信息。没有统一抽象，两者只能各自
在散落的谓词里打补丁，违反根因修复与设计完整性原则。

本 PR **先把抽象立起来**：一个 `Conversion.Classify(from, to) → {kind, method?}` 分类器，
集中所有转换判定并给每种转换**打上正确的种类标签**（窄化=ExplicitNumeric、有损浮点=
ExplicitNumeric、装箱=Boxing……）。**但执行门保持现状宽松**——`_isAssignable` 的布尔投影与
今天逐位相同，因此**产物逐字节不变**，用自举字节不动点（gen1==gen2）+ 全 golden 不变验证。

于是 PR2 的收紧退化成"把门从『接受 ExplicitNumeric』改成『拒绝』"的一处开关，PR3 的用户
转换退化成"给分类器加 UserImplicit/UserExplicit 两个种类 + 一个方法查找"。

## What Changes

- **新增** `Conversion.z42`：`ConvKind`（种类常量）+ `ConvResult`（kind + 可选 method）+
  `Conversion.Classify(from, to, symbols)`。分类**正确**（窄化/有损标 ExplicitNumeric），但不改行为。
- **改** `TypeFactsTc._isAssignable`：改为 `Conversion.Classify(...)` 的布尔投影
  `ConvResult.ImplicitOkPermissive()`——**PR1 阶段该投影包含 ExplicitNumeric**，等价于今天的
  "信任程序员放行数值窄化"，保证零行为变化。
- **零** 新 IR 指令 / 零格式 bump / 零运行期改动 / 零新诊断。cast 绑定、`BoxIfNeeded`、
  codegen 全部**不动**（它们在 PR2/PR3 才改走分类器，因为那时才有行为变化）。
- 单测 + 机制文档。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/Conversion.z42` | NEW | 分类器：ConvKind / ConvResult / Classify |
| `src/compiler/z42c.semantics/src/TypeFactsTc.z42` | MODIFY | `_isAssignable` 改为 Classify 的布尔投影（byte-identical） |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件表登记 Conversion.z42 |
| `src/compiler/z42c.semantics/tests/conversion/conversion_tests.z42` | NEW | 分类器单测（种类标签 + 布尔投影） |
| `src/compiler/z42c.semantics/tests/conversion/z42c.semantics.test.conversion.z42.toml` | NEW | 测试工程清单 |
| `docs/book/src/compiler/type-conversion.md` | NEW | 转换分类器机制页（分类矩阵 + 三 PR 演进路线） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新页 |

**只读引用**（理解上下文，不修改）：

- `src/compiler/z42c.semantics/src/Z42Type.z42` — 现有 `IsAssignableTo` / `_canWiden` 规则来源
- `src/compiler/z42c.semantics/src/TypeChecker.z42` — `BoxIfNeeded` / cast 绑定调用点（PR1 不改）
- `src/compiler/z42c.semantics/src/ExprTyper.z42` — cast/赋值绑定（PR1 不改）
- `src/compiler/z42c.semantics/src/OverloadResolver.z42` — `_assignable` 调用点（PR1 不改）
- `src/compiler/z42c.semantics/tests/types/type_tests.z42` — 单测写法模板

## Out of Scope

- **规则收紧**（隐式窄化→显式、有损浮点→显式、插 ConvertInstr、迁移 stdlib）→ PR2。
- **用户自定义转换**（`implicit`/`explicit operator`、`(C)x` 语法扩展）→ PR3。
- cast 绑定 / `BoxIfNeeded` / codegen / 运行期 `convert_value` 的任何改动。
- 新诊断码 / 新错误信息（PR1 不拒绝任何今天接受的程序）。

## Open Questions

- 无（PR1 定位与验证方式已在探索阶段与 User 确认：字节不动点纯重构）。
