# Proposal: readonly 字段修饰符 + 优化管线利用（同模块）

## Why
z42 无任何不可变性关键字，优化管线因此无法证明字段不变——`IrOptInfo.IsPure` 硬编码排除
`FieldGet`，CSE/LICM 碰不了字段读，每次循环 / 每处重复的 `this.x` 都重新 load 堆。`readonly`
给优化器一个可信契约：**字段构造后不变** → 同接收者重复读可消重、循环内可外提。这是"用语法机制
喂优化管线"里投入产出比最高的一步（自足、不需跨过程分析）。

## What Changes
- **语法**：`readonly` 字段修饰符（新 token、进 `FieldDecl.Mods`）。
- **类型检查**：readonly 字段**只能**在声明类的构造函数体内（`this.<field> = ...`）或字段初始化器
  赋值，其余位置赋值报 `E0415`。
- **IR 内存标志**：`FieldGetInstr.Readonly`（**不序列化到 zbc**，纯编译期优化提示）；
  `FieldSymbol.IsReadonly`（本地符号收集填）；ExprEmitter emit FieldGet 时从字段符号读 readonly。
- **优化管线**（新 OptSet 位 `ReadonlyLoad`，进 `Opt.All`）：
  - 块内 CSE：同接收者 + 同 readonly 字段的重复 `FieldGet` 消重（遇同字段 `FieldSet` 失效值号）。
  - LICM 外提：接收者是 `this` 的 readonly `FieldGet` 提出自然循环。
- **测试**：codegen IrDump 前后对比 + 运行时 golden + bench 前后性能对比。
- **文档**：`docs/book/` 优化管线页 +（新）readonly 语言页；`docs/features.md`。

## 关键设计决策（详见 design.md）
1. readonly 信息用 **FieldGetInstr 内存 bool**（不入 zbc）→ **无格式 bump、无两代自举**。
2. **v1 只做同模块 readonly**；跨 zpkg imported 字段保守当非 readonly（跨包需格式 bump，Deferred）。
3. **LICM 只提接收者 = `this` 的读**（`this` 恒非空 → 无 NPE 时机漂移）；params/locals Deferred。
4. **自举字节不动点不破**：z42c / stdlib 源不用 readonly（support-first），新 opt 对其输出零影响。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | 加 `Readonly = 150` |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | `_initKeywords()` 注册 `readonly` |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | `_isModifier()` 加 Readonly |
| `src/compiler/z42c.semantics/src/Symbol.z42` | MODIFY | `FieldSymbol` 加 `IsReadonly` |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | 本地字段填 `IsReadonly`（从 Mods） |
| `src/compiler/z42c.semantics/src/TypeEnv.z42` | MODIFY | 加"当前在 ctor 内（this 接收者）"标志 |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 绑 ctor 体时置 ctor 标志 |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindAssign` 查字段 readonly + ctor 上下文 → E0415 |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | `ReadonlyAssignment = "E0415"` |
| `src/libraries/z42.ir/src/IrInstr.z42` | MODIFY | `FieldGetInstr` 加内存 `Readonly`（不序列化） |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | emit FieldGet 填 readonly（从 FieldSymbol） |
| `src/compiler/z42c.semantics/src/OptSet.z42` | MODIFY | `ReadonlyLoad = 256`，`All = 511`，名字映射 |
| `src/compiler/z42c.semantics/src/IrOptInfo.z42` | MODIFY | `CseKey` 加 readonly-FieldGet 分支 |
| `src/compiler/z42c.semantics/src/IrOptPipeline.z42` | MODIFY | CSE pass：readonly FieldGet 消重 + FieldSet 失效 |
| `src/compiler/z42c.semantics/src/IrLicm.z42` | MODIFY | this 接收者 readonly FieldGet 外提 |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | readonly-CSE / LICM IrDump 对比用例 |
| `src/compiler/z42c.semantics/tests/typecheck/typecheck_tests.z42` | MODIFY | readonly 赋值合法/非法诊断用例 |
| `src/tests/optimization/readonly-field-hoist/` | NEW | 运行时 golden（source.z42 + expected_output.txt） |
| `src/libraries/z42.core/bench/readonly_field_bench.z42` | NEW | bench 前后对比 fixture |
| `docs/book/src/runtime/optimization-pipeline.md` | MODIFY | readonly-load pass 机制 |
| `docs/book/src/language/readonly-fields.md` | NEW | 语言页（挂入 SUMMARY.md） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂新页 |
| `docs/features.md` | MODIFY | 登记 readonly + phase 归属 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 登记跨包 readonly / 非空 LICM |

**只读引用**：`Decl.z42`（FieldDecl 结构）、`Bound.z42`（BoundMember/BoundAssign）、
`ClassDescBuilder.z42`（IrFieldDesc 构造，仅参考不改——不走 zbc 路径）、`IrDump.z42`（DumpFuncOpt）。

## Out of Scope（Deferred，登记 roadmap）
- **跨 zpkg imported 字段 readonly**：需 zbc/zpkg 格式 bump（IrFieldDesc + ZbcWriter/Reader +
  TsigReconcile + ImportedSymbolLoader）+ 两代自举。v1 imported 字段保守当非 readonly。
- **非 `this` 接收者的 LICM**（params/locals）：需非空或支配分析证明无 NPE 时机漂移。
- `readonly struct` / `pure` 方法标注 / non-null 引用类型：各自独立 change。

## Open Questions
- 无（6.5 gate 已确认三点：同模块 only / LICM 仅 this / 独立 OptSet 位）。
