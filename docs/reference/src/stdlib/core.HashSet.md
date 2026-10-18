<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.HashSet

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

[`src/libraries/z42.core/src/Collections/HashSet.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L9)

插入有序的泛型哈希集合（与 `Dictionary<TKey, TValue>` 同一布局，共用操作见 `HashIndex.z42`）：摊还 O(1) 平均 Add / Remove / Contains。`ToArray()` 按**插入顺序**产出； 已存在元素再 `Add` 不改变位置；`Remove` 后再加入排到末尾。`T` 必须 hashable + equatable （primitive 走 VM builtin，用户类型实现 `IEquatable`），与 Dictionary 的 TKey 约束一致。

没有 `GetEnumerator()`，不能直接 `foreach` —— 用 `ToArray()` 取快照后再遍历。

```z42
public class HashSet<T> where T : IEquatable {
    int Count
    HashSet()
    bool IsEmpty()
    bool Add(T item)
    bool Contains(T item)
    bool Remove(T item)
    void Clear()
    T[] ToArray()
    void UnionWith(T[] items)
    void IntersectWith(T[] items)
    void ExceptWith(T[] items)
}
```

成员：[`Count`](#Std.Collections.HashSet%3CT%3E.Count) · [`HashSet`](#Std.Collections.HashSet%3CT%3E.ctor()) · [`IsEmpty`](#Std.Collections.HashSet%3CT%3E.IsEmpty()) · [`Add`](#Std.Collections.HashSet%3CT%3E.Add(T)) · [`Contains`](#Std.Collections.HashSet%3CT%3E.Contains(T)) · [`Remove`](#Std.Collections.HashSet%3CT%3E.Remove(T)) · [`Clear`](#Std.Collections.HashSet%3CT%3E.Clear()) · [`ToArray`](#Std.Collections.HashSet%3CT%3E.ToArray()) · [`UnionWith`](#Std.Collections.HashSet%3CT%3E.UnionWith(T[])) · [`IntersectWith`](#Std.Collections.HashSet%3CT%3E.IntersectWith(T[])) · [`ExceptWith`](#Std.Collections.HashSet%3CT%3E.ExceptWith(T[]))

<a id="Std.Collections.HashSet<T>.Count"></a>
### int Count

[`src/libraries/z42.core/src/Collections/HashSet.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L16)

元素个数（公开字段，可读可写）。

<a id="Std.Collections.HashSet<T>.ctor()"></a>
### HashSet()

[`src/libraries/z42.core/src/Collections/HashSet.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L19)

新建空集合。

<a id="Std.Collections.HashSet<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.core/src/Collections/HashSet.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L30)

是否为空。

<a id="Std.Collections.HashSet<T>.Add(T)"></a>
### bool Add(T item)

[`src/libraries/z42.core/src/Collections/HashSet.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L33)

加入元素；已存在返回 false（不重复、位置不变），新增返回 true 并追加到末尾。

<a id="Std.Collections.HashSet<T>.Contains(T)"></a>
### bool Contains(T item)

[`src/libraries/z42.core/src/Collections/HashSet.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L66)

是否含元素。

<a id="Std.Collections.HashSet<T>.Remove(T)"></a>
### bool Remove(T item)

[`src/libraries/z42.core/src/Collections/HashSet.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L86)

删除元素；删掉返回 true，不存在返回 false。其余元素相对顺序不变。

<a id="Std.Collections.HashSet<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.core/src/Collections/HashSet.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L100)

清空集合，保留容量。

<a id="Std.Collections.HashSet<T>.ToArray()"></a>
### T[] ToArray()

[`src/libraries/z42.core/src/Collections/HashSet.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L103)

元素快照数组，按插入顺序。

<a id="Std.Collections.HashSet<T>.UnionWith(T[])"></a>
### void UnionWith(T[] items)

[`src/libraries/z42.core/src/Collections/HashSet.z42:124`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L124)

并入 items 的全部元素（并集，等价 C# UnionWith）：新元素按 items 顺序追加。

<a id="Std.Collections.HashSet<T>.IntersectWith(T[])"></a>
### void IntersectWith(T[] items)

[`src/libraries/z42.core/src/Collections/HashSet.z42:129`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L129)

只保留同时存在于 items 的元素（交集，等价 C# IntersectWith）；保留下来的元素顺序不变。

<a id="Std.Collections.HashSet<T>.ExceptWith(T[])"></a>
### void ExceptWith(T[] items)

[`src/libraries/z42.core/src/Collections/HashSet.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/HashSet.z42#L139)

移除 items 中出现的元素（差集，等价 C# ExceptWith）。

