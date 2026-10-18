<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DictionaryEnumerator

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

[`src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42#L8)

`Dictionary<TKey, TValue>` 的枚举器：按 entries 顺序（= 插入顺序）遍历，跳过 tombstone。

首次 `MoveNext` 记下字典的 `_version`；此后字典新增键 / `Clear` / 压实会改变它，下一次
`MoveNext` 即抛 `InvalidOperationException`（对齐 C# 的 "Collection was modified"）。
覆盖已有键与 `Remove` 不改 `_version`：entry 原位不动，枚举照常继续（被删项不再产出）。

```z42
public struct DictionaryEnumerator<TKey,TValue> where TKey : IEquatable {
    Dictionary<TKey,TValue> _dict
    DictionaryEnumerator(Dictionary<TKey,TValue> _dict)
    bool MoveNext()
    KeyValuePair<TKey,TValue> Current { get; }
    void Dispose()
}
```

成员：[`_dict`](#Std.Collections.DictionaryEnumerator%3CTKey,TValue%3E._dict) · [`DictionaryEnumerator`](#Std.Collections.DictionaryEnumerator%3CTKey,TValue%3E.ctor(Dictionary%3CTKey,TValue%3E)) · [`MoveNext`](#Std.Collections.DictionaryEnumerator%3CTKey,TValue%3E.MoveNext()) · [`Current`](#Std.Collections.DictionaryEnumerator%3CTKey,TValue%3E.Current) · [`Dispose`](#Std.Collections.DictionaryEnumerator%3CTKey,TValue%3E.Dispose())

<a id="Std.Collections.DictionaryEnumerator<TKey,TValue>._dict"></a>
### Dictionary<TKey,TValue> _dict

[`src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42#L8)


<a id="Std.Collections.DictionaryEnumerator<TKey,TValue>.ctor(Dictionary<TKey,TValue>)"></a>
### DictionaryEnumerator(Dictionary<TKey,TValue> _dict)

[`src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42#L8)


<a id="Std.Collections.DictionaryEnumerator<TKey,TValue>.MoveNext()"></a>
### bool MoveNext()

[`src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42#L15)

前进到下一个存活 entry；无更多返回 false，字典被结构性修改抛 `InvalidOperationException`。

<a id="Std.Collections.DictionaryEnumerator<TKey,TValue>.Current"></a>
### KeyValuePair<TKey,TValue> Current { get; }

[`src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42#L37)

当前键值对。

<a id="Std.Collections.DictionaryEnumerator<TKey,TValue>.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42#L42)

无操作（值类型枚举器，无资源）。

