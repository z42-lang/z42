# Proposal: Std.Array range / offset / paired 重载补齐

## Why

`Std.Array`（`add-array-csharp-algorithms` / #380）已补齐 C# `System.Array` 的谓词搜索、
二分查找、变换类静态算法，但仍缺 C# BCL 里常用的 **range（子区间）/ offset（偏移）/ paired（键值
配对）重载**。这些是纯脚本泛型算法（同 #380 模型：`CompareTo` / `Predicate` / `Func` 比较器），
不涉及新语言特性，属既有 overload 决议能力可承载的 parity 补齐。不做则用户在只想操作数组一段、
或按 keys 排序 items 时无对应 API，需自己手写。

## What Changes

在 `Std.Array` 新增以下静态泛型重载（全部纯脚本）：

- **range 排序**：`Sort<T>(T[], int index, int length)`
- **range 反转**：`Reverse<T>(T[], int index, int length)`
- **range 填充**：`Fill<T>(T[], T value, int startIndex, int count)`
- **偏移拷贝**：`Copy<T>(T[] src, int srcIdx, T[] dst, int dstIdx, int length)`（含同数组重叠区正确处理）
- **range 正向查找**：`IndexOf<T>(T[], T, int startIndex)` / `IndexOf<T>(T[], T, int startIndex, int count)`
- **range 逆向查找**：`LastIndexOf<T>(T[], T, int startIndex)`
- **range / comparer 二分**：`BinarySearch<T>(T[], int index, int length, T value)` /
  `BinarySearch<T>(T[], T value, Func<T, T, int> comparison)`

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Array.z42` | MODIFY | 新增上列重载 + 私有辅助 `_checkRange` |
| `src/libraries/z42.core/tests/array_range_overloads.z42` | NEW | `[Test]` 用例覆盖每个重载的正常 + 边界/异常 |
| `src/libraries/z42.core/README.md` | MODIFY | 功能索引 Array 行同步新重载 |
| `docs/spec/changes/add-array-range-overloads/` | NEW | 本变更容器（proposal/design/spec/tasks） |

**只读引用**：

- `src/libraries/z42.core/src/Delegates/Delegates.z42` — 确认 `Func`/`Predicate`/`Action` 定义
- `docs/spec/archive/2026-09-02-add-array-csharp-algorithms/` — 参照 #380 的算法风格与命名

## Out of Scope

- **配对排序 `Sort<TKey,TValue>(TKey[], TValue[])`** —— 实测与既有 `Sort<T>(T[], Func<T,T,int>)`
  （同为 2 参、类型参数个数不同）在当前 overload 决议器下**无法干净共存**：加入后 `Sort<int>(arr, lambda)`
  被误绑（既有 comparator 排序测试变成升序、丢 comparator）。修复需动编译器 overload 决议（超出 stdlib
  scope），拆为独立 change 处理（届时先修/评估决议器对「同参数数、不同类型参数个数」重载的选择）。
- `AsReadOnly<T>` —— 需先建 `ReadOnlyCollection<T>` 包装类，是独立一块，另开 change。
- 多维数组 `T[,]` 相关 API（`Rank`/`GetLength(dim)` 等）—— z42 当前不支持多维数组类型，属 lang 大特性。
- 泛型数组值类型零初始化根修（`new T[n]` 未写槽 Null）—— 独立 vm change（本批新增算法均不读 `new T[n]` 未写槽，不踩该坑）。

## Open Questions

- [x] `Sort<TKey,TValue>(TKey[], TValue[])` 与 `Sort<T>(T[], Func<T,T,int>)` 决议歧义——**已证实歧义**，
  paired Sort 移出本 change（见 Out of Scope）。其余重载均按 arg-count 与既有重载区分，无歧义。
