# Proposal: Std.Array 补齐 C# System.Array 静态算法

## Why
`Std.Array` 目前的静态算法只有 `Sort` / `IndexOf` / `Copy` / `Fill` / `Reverse`（library-review PR-D）。
C# `System.Array` 常用的谓词查找（`Find` / `Exists` / `TrueForAll` …）、二分查找、映射（`ConvertAll`）、
范围清零（`Clear`）等仍缺失，用户处理裸数组时不得不手写循环。这是「Array 尽量向 C# 看齐、算法在脚本侧
实现」方向的自然延续——真原语（`GetValue`/`SetValue`/`Clone`/`CreateInstance`/`Length`）保持 native，
算法用纯 z42 脚本基于既有原语与委托（`Predicate<T>`/`Func<T,R>`/`Action<T>`）补齐。

## What Changes
给 `Std.Array` 新增 15 个泛型静态方法（对标 `System.Array`，纯脚本实现）：

- **谓词查找**：`Find` / `FindLast` / `FindIndex` / `FindLastIndex` / `FindAll` / `Exists` / `TrueForAll`
- **查找**：`BinarySearch`（有序数组，`CompareTo`，缺失时返回 `~插入点`，C# 语义）/ `Contains` / `LastIndexOf`
- **变换 / 工具**：`ConvertAll` / `ForEach` / `Clear`（范围置 `default(T)`）/ `Resize`（返回新数组）/ `Empty`

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Array.z42` | MODIFY | 追加 15 个泛型静态算法方法 |
| `src/libraries/z42.core/tests/array_csharp_algorithms.z42` | NEW | 新方法的 `[Test]` 单元用例（正常 + 边界）|
| `src/libraries/z42.core/README.md` | MODIFY | 功能索引 Array 行补列新算法 |
| `docs/spec/changes/add-array-csharp-algorithms/*` | NEW | 本变更容器（归档随 PR）|

**只读引用**：

- `src/libraries/z42.core/src/Delegates/Delegates.z42` — `Predicate<T>` / `Func<T,R>` / `Action<T>` 定义
- `src/libraries/z42.core/src/Collections/List.z42` — 既有 `Sort`/`IndexOf` 脚本风格参考
- `src/libraries/z42.core/tests/array_algorithms.z42` — 既有 `[Test]` 风格参考

## Out of Scope
- 不动 native 原语（`GetValue`/`SetValue`/`Clone`/`CreateInstance`/`Length`）——它们不可用纯脚本表达。
- 不改 `List<T>`（本变更只补 `Array`）。
- 多维数组（`Rank`/`GetLength(dim)`）、`Array.Sort` 的 key-value 重载、`AsReadOnly` 暂不做。

## Open Questions
- 无（范围与 `Resize` 签名已由 User 裁决：全套 C# 对齐 + `Resize` 返回新数组）。
