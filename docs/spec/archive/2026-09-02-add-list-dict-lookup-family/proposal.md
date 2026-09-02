# Proposal: List/Dictionary 查找族补齐（C# corelib 对齐）

## Why
z42.core 的 `List<T>` / `Dictionary<TKey,TValue>` 缺少 C# `System.Collections.Generic`
里最常用的一批查询/查找成员（`Find*` / `Exists` / `TrueForAll` / `RemoveAll` /
`BinarySearch` / `GetValueOrDefault` / `TryAdd` 等），用户写集合查询要手写循环。补齐这批
纯 additive 成员是「推进 corelib 对齐」程序 backlog #2。

## What Changes
- `List<T>`：新增查询/谓词族 `Find` / `FindLast` / `FindIndex` / `FindLastIndex` /
  `FindAll` / `Exists` / `TrueForAll` / `RemoveAll` / `LastIndexOf` / `GetRange` /
  `BinarySearch`。为保持文件可读，`List<T>` 拆成 partial：核心留 `List.z42`，查询族入
  新文件 `List.Query.z42`。
- `Dictionary<TKey,TValue>`：新增 `TryAdd` / `GetValueOrDefault(key)` /
  `GetValueOrDefault(key, default)`。
- 均为纯 additive，无既有签名/行为变更、无格式变更。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Collections/List.z42` | MODIFY | `class`→`partial class`，加拆分说明注释 |
| `src/libraries/z42.core/src/Collections/List.Query.z42` | NEW | List 查询/谓词族 partial 第二部分 |
| `src/libraries/z42.core/src/Collections/Dictionary.z42` | MODIFY | 加 TryAdd / GetValueOrDefault×2 |
| `src/libraries/z42.core/tests/list_query.z42` | NEW | List 查询族 [Test] dogfood |
| `src/libraries/z42.core/tests/dictionary_lookup.z42` | NEW | Dictionary 查找族 [Test] dogfood |
| `src/libraries/z42.core/README.md` | MODIFY | 功能索引 + List 尺寸例外 + Deferred |

**只读引用**：
- `src/libraries/z42.core/src/Delegates/Delegates.z42` — `Predicate<T>`/`Func` 声明
- `src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42` — 评估 AsReadOnly 可行性

## Out of Scope
- `List<T>.ConvertAll<TOut>`（方法级泛型约束传播，Deferred）
- `List<T>.AsReadOnly`（活视图 vs 快照语义待决，Deferred）
- `Dictionary.TryGetValue`（依赖 `out`，待 out→tuple 迁移，Deferred）
- `Dictionary.ContainsValue`（TValue 无约束，值相等装箱路径待评估，Deferred）
- `HashSet<T>`（backlog #4，独立变更）

## Open Questions
- 无（List 尺寸例外 + TryGetValue 处理已由 User 裁决）。
