# Proposal: 推断 var 字段类型（infer-var-field-types）

> 状态：🟡 进行中 | 创建：2026-07-24 | 类型：`fix`（编译器根因，产出端）
> 子系统：`compiler`（z42c.semantics）
> 下游：`add-z42-repl`（REPL 跨轮 carry-forward 状态模型依赖它——见该 change design D8）

## Why

**通用语言缺陷**：`public static var x = 5;` 这类推断类型字段，**跨 zpkg 导出时丢失推断类型**
（导出为语法「var」），导致任何跨包类型相关消费失败：
```
// pkgB 引用 pkgA 的 public static var 字段
return (PkgA.Vars.x + 100);   // E0402: operator + requires numeric operand, got var
```

**根因（已定位）**：`SymbolCollector.z42:576` 对 var 字段 `ResolveTypeP(fd.Type)` → 得
Unknown/「var」占位存进 `FieldSymbol.FieldType`，**从不从初始化器推断**。
- 同包访问「能编」只是 `Z42UnknownType` 被算术检查宽松放过（运行期字节码仍是正确的原始类型）；
- 跨包导入成具名「var」类型 → 严格数值检查 → `E0402`。

即：字段类型在符号层降级为「var」，同包消费端靠宽松/重推断掩盖，跨包消费端无从掩盖。
**违反 philosophy.md「跨阶段类型降级 → Phase 2 fixup pass 升级回，禁止消费端容错」**——本 change 补上那个 fixup。

## What Changes

**两条跨包导出通道都要修**（实施期实证：只修 TSIG 无效——跨包解析读 TYPE 段，不读 TSIG）：

1. **fixup pass（TSIG 通道）**：`IrDump.BuildPackageCus` **SymbolCollect 后、Export 前**插入
   `VarFieldInfer.Run`：对每个 static var 字段，用 `TypeChecker._expr._bindExpr(init, TypeEnv.Root(symbols))`
   （throwaway 诊断）绑定初始化器取类型，若 `fs.FieldType` 是 `Z42UnknownType` 则回写。链式引用
   （`var y = C.x * 2`）**迭代到 fixpoint**（≤8 轮）。→ Export 读 `fs.FieldType`
   （`ExportedTypeExtractor.z42:366`）即写真实类型；同包 `MemberResolver` 也收敛真实类型。

2. **TYPE 段通道**：`ClassDescBuilder.z42:141` 原用 `_typeFieldName(fd.Type)`（AST 源拼写→var 得「var」）
   → 改为 **var 字段用推断后的 `fs.FieldType.Name()`**（非 var 字段保留源拼写不动，保 `byte[]` 语义）。
   跨包解析（DepScan/TsigReconcile 读 TYPE/SIGS 段，drop-tsig-expt 后）由此得真实类型。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.semantics/src/VarFieldInfer.z42` | NEW | fixup pass：绑定 static var 字段初始化器 → 推断 → 回写 fs.FieldType（fixpoint）|
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | BuildPackageCus 插入 `VarFieldInfer.Run`（SymbolCollect 后、Export 前）|
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | TYPE 段字段类型：var 字段用推断后 fs.FieldType（非 var 不动）|
| `src/tests/cross-zpkg/var_field_cross_pkg/` | NEW | e2e fixture：跨包 var 字段算术(105) + 字符串拼接(z42!) |
| `docs/spec/changes/infer-var-field-types/{proposal,tasks}.md` | NEW | 本 change 文档 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 compiler 占用（独立分支） |

**只读引用**：`SymbolCollector.z42:576`（根因）、`ExportedTypeExtractor.z42:366`（TSIG 读 FieldType）、
`DeclBinder.z42:209`（静态 init 绑定范式）、`ExprTyper.z42:326`（_bindExpr）。

## Out of Scope
- 实例 var 字段（`_injectFieldInits` 路径）——本 change 聚焦 static（REPL 只用 static）；实例 var 字段
  跨包同缺陷但另立 follow-up（`infer-var-field-types-instance`）若有需求。
- 复杂前向依赖无法在 fixpoint 内收敛的 var 字段 → 保持 Unknown（不回归，等同现状）。

## GREEN 判据
- 跨包 static var 字段算术编译通过（新单测 + 验证回路实证）。
- **self-host 7/7 gen1==gen2 逐字节不变**（z42c 源 0 处 public static var 字段 → 零字节漂移，硬证明无回归）。
- stdlib 全 [Test] 绿 + z42c 单测绿（0 处 public static var → 零影响）。
