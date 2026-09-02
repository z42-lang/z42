# Proposal: `ReadOnlyCollection<T>` + `Array.AsReadOnly<T>`

> 状态：进行中 | 创建：2026-09-02 | 类型：stdlib feat（纯脚本，无编译器/VM 改动）

## What

为 `z42.core` 增加只读集合支持，向 C# `System.Array.AsReadOnly` /
`System.Collections.ObjectModel.ReadOnlyCollection<T>` 看齐：

- 新类型 `Std.Collections.ReadOnlyCollection<T>`（`Collections/ReadOnlyCollection.z42`）——
  按引用包装一个 `T[]` 的只读视图。
- 新静态方法 `Std.Array.AsReadOnly<T>(T[])`（`Array.z42`）——返回 `ReadOnlyCollection<T>`。

## Why

承接 [[array-csharp-parity-followup]]（口令「推进数组 API」）里 `System.Array` 补齐主题的剩余项。
`AsReadOnly` 在 #382（range/offset 重载）时被列为「需先建 `ReadOnlyCollection<T>` 包装类」而移出，
本 change 补上该包装类并接通 `AsReadOnly`。

只读视图是把可变数组安全暴露给调用方的惯用手段（对齐 C# BCL），无需复制底层数据。

## Scope

**纯脚本 stdlib feat**：只加两处源码 + 测试 + 文档，不碰 lang / IR / VM，无格式 bump。

### In scope
- `ReadOnlyCollection<T>`：`Count`、只读索引器 `this[int] { get }`、`Contains`、`IndexOf`、
  `CopyTo`、`ToArray`、foreach 支持。
- `Array.AsReadOnly<T>(T[])`。

### Out of scope
- **只读接口层次**（`IReadOnlyList<T>` / `IReadOnlyCollection<T>` / `ICollection<T>` / `IList<T>`）——
  z42.core 目前**均无**这些接口（仅有 `IEnumerable<T>` / `IEnumerator<T>`）。C# 的 `ReadOnlyCollection`
  实现 `IList<T>` 等一大票接口并在变更方法上抛 `NotSupportedException`；本 change **不引入接口层次**，
  只读契约由「不提供变更 API」在类型层面保证（无 `Add`/`set` 索引器）。日后若建只读接口族再补 `implements`。
- `List<T>.AsReadOnly()`（实例方法）——可后续加，先做 `Array.AsReadOnly`。
- `IEnumerable<T>` 实现 / `GetEnumerator`——不需要：foreach 走 z42 索引快路径（`Count` + get 索引器）。
