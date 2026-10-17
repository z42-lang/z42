<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.KeyValuePair

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

`src/libraries/z42.core/src/Collections/KeyValuePair.z42:7`

不可变键值对，主要用作 `Dictionary<TKey, TValue>.Entries()` 的元素类型。

`[Record] struct` 值类型：无堆分配的 blob 布局，`Key` / `Value` 创建后不可变，
值相等 / 哈希 / `ToString` 由编译器合成（对齐 C# `System.Collections.Generic.KeyValuePair<,>`）。

```z42
public struct KeyValuePair<TKey,TValue> {
    TKey Key
    TValue Value
    KeyValuePair(TKey Key, TValue Value)
}
```

### TKey Key

`src/libraries/z42.core/src/Collections/KeyValuePair.z42:7`


### TValue Value

`src/libraries/z42.core/src/Collections/KeyValuePair.z42:7`


### KeyValuePair(TKey Key, TValue Value)

`src/libraries/z42.core/src/Collections/KeyValuePair.z42:7`


