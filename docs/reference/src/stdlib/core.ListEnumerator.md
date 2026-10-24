<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ListEnumerator

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

[`src/libraries/z42.core/src/Collections/ListEnumerator.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/ListEnumerator.z42#L10)

`List<T>` 的前向迭代器。`[Record] struct` 值类型：无堆分配，对齐 C# `List<T>.Enumerator`。

`_pos` = 已产出元素计数（0 初值）。`MoveNext` 先自增再判界、`Current` 读 `_list[_pos-1]`，
等价 C# 的「-1 起、先自增后读」。

`List<T>` 自身 foreach 走索引快路径（`Count` + `get_Item`），不经本枚举器；本类服务以
`IEnumerable<T>` 接口静态类型 / 泛型约束持有的调用点。

```z42
public struct ListEnumerator<T> : IEnumerator<T> {
    List<T> _list
    ListEnumerator(List<T> _list)
    bool MoveNext()
    T Current { get; }
    void Dispose()
}
```

成员：[`_list`](#Std.Collections.ListEnumerator%3CT%3E._list) · [`ListEnumerator`](#Std.Collections.ListEnumerator%3CT%3E.ctor(List%3CT%3E)) · [`MoveNext`](#Std.Collections.ListEnumerator%3CT%3E.MoveNext()) · [`Current`](#Std.Collections.ListEnumerator%3CT%3E.Current) · [`Dispose`](#Std.Collections.ListEnumerator%3CT%3E.Dispose())

<a id="Std.Collections.ListEnumerator<T>._list"></a>
### List<T> _list

[`src/libraries/z42.core/src/Collections/ListEnumerator.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/ListEnumerator.z42#L12)

被枚举的列表。

<a id="Std.Collections.ListEnumerator<T>.ctor(List<T>)"></a>
### ListEnumerator(List<T> _list)

[`src/libraries/z42.core/src/Collections/ListEnumerator.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/ListEnumerator.z42#L10)

`List<T>` 的前向迭代器。`[Record] struct` 值类型：无堆分配，对齐 C# `List<T>.Enumerator`。

`_pos` = 已产出元素计数（0 初值）。`MoveNext` 先自增再判界、`Current` 读 `_list[_pos-1]`，
等价 C# 的「-1 起、先自增后读」。

`List<T>` 自身 foreach 走索引快路径（`Count` + `get_Item`），不经本枚举器；本类服务以
`IEnumerable<T>` 接口静态类型 / 泛型约束持有的调用点。

<a id="Std.Collections.ListEnumerator<T>.MoveNext()"></a>
### bool MoveNext()

[`src/libraries/z42.core/src/Collections/ListEnumerator.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/ListEnumerator.z42#L16)

前进到下一个元素；无更多返回 false。

<a id="Std.Collections.ListEnumerator<T>.Current"></a>
### T Current { get; }

[`src/libraries/z42.core/src/Collections/ListEnumerator.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/ListEnumerator.z42#L25)

当前元素。

<a id="Std.Collections.ListEnumerator<T>.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Collections/ListEnumerator.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/ListEnumerator.z42#L30)

无操作（值类型枚举器，无资源）。

