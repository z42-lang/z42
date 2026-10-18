<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IComparer

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Protocols/IComparer.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IComparer.z42#L10)

外部比较器契约：接收两个对象做大小比较。

与 IComparable 的「自比」模式区分：
  - IComparable.CompareTo(other) — 实例知道如何与自己比另一个
  - IComparer<T>.Compare(x, y)   — 比较器接收两个外部对象

用于 stdlib Sort 重载 / SortedDictionary 的自定义排序契约。

```z42
public interface IComparer<T> {
    int Compare(T x, T y)
}
```

成员：[`Compare`](#Std.IComparer%3CT%3E.Compare(T,T))

<a id="Std.IComparer<T>.Compare(T,T)"></a>
### int Compare(T x, T y)

[`src/libraries/z42.core/src/Protocols/IComparer.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IComparer.z42#L12)

比较 x 与 y：负表示 x 排在 y 之前，0 表示等价，正表示 x 排在 y 之后。

