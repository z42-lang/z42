<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ReadOnlyCollection

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:9`

对一个既有 `T[]` 的只读视图（对标 C# `System.Collections.ObjectModel.ReadOnlyCollection<T>`）。

包装**按引用，不复制**：对底层数组的读通过本视图可见；只读由「没有任何变更成员」在类型层面
保证，而非运行期抛异常。由 `Array.AsReadOnly<T>` 构造，也可直接 `new`。有 `Count` + get-only
索引器 ⇒ 支持 foreach。无泛型约束：Contains / IndexOf 用 `Object.Equals`，任意 `T[]` 都能包装。

```z42
public sealed class ReadOnlyCollection<T> {
    int Count
    ReadOnlyCollection(T[] array)
    T this[int index] { get; }
    bool Contains(T value)
    int IndexOf(T value)
    void CopyTo(T[] array, int arrayIndex)
    T[] ToArray()
}
```

### int Count

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:13`

元素个数（公开字段）。

### ReadOnlyCollection(T[] array)

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:16`

包装既有数组（按引用，不复制）。array 不应为 null。

### T this[int index] { get; }

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:22`

只读索引访问；越界由底层数组抛 IndexOutOfRangeException。

### bool Contains(T value)

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:27`

是否包含 value（按 Object.Equals 线性查找）。

### int IndexOf(T value)

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:35`

首个等于 value 的下标（按 Object.Equals），未命中返回 -1。

### void CopyTo(T[] array, int arrayIndex)

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:43`

将全部元素拷贝到 array，从 arrayIndex 处起。

### T[] ToArray()

`src/libraries/z42.core/src/Collections/ReadOnlyCollection.z42:50`

返回元素的新数组副本（防御性快照）。

