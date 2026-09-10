# DRAFT: unify-resolved-vs-source-type-spelling —— 消灭「已解析名 vs 源拼写」两套类型名口径

> 状态：📝 DRAFT（未开工）| 创建：2026-09-11 | 类型：**lang/ir**（改持久化内容语义 → 触发格式 bump）
> 出身：`type-identity-followups` 里原定的两项（实参拼写统一 + 格式 bump）合并升级而来。
> 前置：[unify-type-identity-fqn](../../archive/2026-09-10-unify-type-identity-fqn/)（#559，已合并）

## 问题

**同一个类型在持久化产物里有两种拼写**，取决于由哪个产出方写出：

| 口径 | 产出方 | 例 |
|---|---|---|
| **源拼写** | `ClassDescBuilder._typeSourceName`（TYPE 段） | `List<int>` |
| **已解析名** | `FunctionEmitter._sigTypeName`（SIGS）、`StubEmitter._typeSpell`、IR 指令操作数 | `List<Int32>` |

后果就是欠债扫描里的 **R7：`Func<int>` 与 `Func<Int32>` 判不相等** —— 同一类型两个名字，
消费端按名比较时失配。这与 #559 修掉的「短名不是唯一键」是同一族缺陷的另一半。

## 🔴 已实测的事实（2026-09-11，别重走这些弯路）

1. **不能只改一个产出方。** 把 `_sigArgTypeName` 的兜底从 `t.Name()` 改成
   `PrimModel.SurfaceName(t.Name())`（只让 SIGS 倒向源拼写口径）会**打断跨包泛型 struct 的解析**：
   `generic_struct_array_cross_pkg` 的 main 整个导入失效。harness 实测**改则红、不改则绿**，
   已隔离到单行。推断机制：消费端按「已解析名」口径建键，只倒一家就失配。
   ⇒ **要改必须四个产出方 + 消费端的键一起改。**

2. **本地编译路径本来就是关键字拼写。** `SymbolTable.ResolveTypeP` 把 `int` 解析成
   `Z42ClassType.Builtin("int")`，其 `Name()` 就是 `"int"` ⇒ 本地写的 `Func<int, int>`
   存下来就是 `Func<int, int>`。**差异只出现在已解析/导入路径**（`int` 在那里是真类 `Std.Int32`）。
   ⚠️ 我为此做了两版单测门、两版都是**空门**（本地路径本就正确，退回改动照样全绿）。
   ⇒ **本 change 的门必须是跨包场景**，单文件/单包单测造不出差异。

3. **全仓受影响面极小**（改前实测）：只有 `Func<T, T, Int32>` 与 `Std.Collections.List<String>`
   两个串。⇒ 价值不在"改这两个串"，而在**消除口径分裂本身**，防止将来再长出 R7 类缺陷。

## 与格式 bump 的关系

**本 change 完成时就是格式 bump 的触发点**，两件事应一次办：

- #559（FQN 化）当时裁决**不 bump**（实测双向互操作正确，且带 bump 拿不到本地全绿——
  格式常量住 `z42.ir`，两代自举是 CI 的活，本地是环境墙）。
- 此后 `type-identity-followups`（E0456 声明位）是**纯诊断、零字节影响**，无可 bump 之物。
- ⇒ 下一次真正改动持久化内容语义的，就是本 change。届时按
  [version-bumping.md](../../../.claude/rules/version-bumping.md) 走 zbc + zpkg 双 bump，
  **两次内容变更（#559 的 FQN + 本次的拼写）合用一次 bump**。

## 落地要点（开工前须自行复核，别照抄）

1. 先把**四个产出方**列全并逐一确认（本文件只列了已知的四个，可能有遗漏——
   用「改一处 → 逐串 diff zpkg STRS」的方式实证，别靠 grep 推断）。
2. 消费端的键（`ImportedSymbolLoader._resolve` 及其调用者）一并归一。
3. 门建在 `src/tests/cross-zpkg/`，**必须做退回对照证明它会红**。
4. 格式 bump 按 checklist 9 步；本地全绿拿不到时以 CI 为准
   （[bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md)）。

## 为什么当初没顺手做完

`type-identity-followups` 原计划把它作为「实参拼写统一」一并做掉，实施中发现：
① 只改一家会炸（事实 1）；② 建不出判别力门（事实 2）；③ 收益面只有 2 个串（事实 3）。
按「不交付解释不了的东西 / 不留没有门盯着的改动」，拆出独立立项。
