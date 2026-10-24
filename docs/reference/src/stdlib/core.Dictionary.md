<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Dictionary

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Collections`

[`src/libraries/z42.core/src/Collections/Dictionary.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L13)

插入有序的泛型哈希映射：摊还 O(1) 平均读写，遍历按**插入顺序**。

遍历（`foreach` / `GetEnumerator` / `Keys()` / `Values()` / `Entries()`）按插入
顺序；覆盖已有键不改变其位置；`Remove` 后再插入同一键排到末尾。`TKey` 必须
hashable + equatable（primitive 走 VM builtin 路由 GetHashCode / Equals；自定义
类型实现 `IEquatable`）。

```z42
public class Dictionary<TKey,TValue> where TKey : IEquatable {
    int Count
    Dictionary()
    bool IsEmpty()
    DictionaryEnumerator<TKey,TValue> GetEnumerator()
    TValue this[TKey key] { get; set; }
    void Set(TKey key, TValue value)
    TValue Get(TKey key)
    bool TryAdd(TKey key, TValue value)
    TValue GetValueOrDefault(TKey key)
    TValue GetValueOrDefault(TKey key, TValue defaultValue)
    bool ContainsKey(TKey key)
    bool Remove(TKey key)
    void Clear()
    TKey[] Keys()
    TValue[] Values()
    KeyValuePair<TKey,TValue>[] Entries()
    int FindSlot(TKey key)
    void Grow()
}
```

成员：[`Count`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Count) · [`Dictionary`](#Std.Collections.Dictionary%3CTKey,TValue%3E.ctor()) · [`IsEmpty`](#Std.Collections.Dictionary%3CTKey,TValue%3E.IsEmpty()) · [`GetEnumerator`](#Std.Collections.Dictionary%3CTKey,TValue%3E.GetEnumerator()) · [`this`](#Std.Collections.Dictionary%3CTKey,TValue%3E.this[TKey]) · [`Set`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Set(TKey,TValue)) · [`Get`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Get(TKey)) · [`TryAdd`](#Std.Collections.Dictionary%3CTKey,TValue%3E.TryAdd(TKey,TValue)) · [`GetValueOrDefault`](#Std.Collections.Dictionary%3CTKey,TValue%3E.GetValueOrDefault(TKey)) · [`GetValueOrDefault`](#Std.Collections.Dictionary%3CTKey,TValue%3E.GetValueOrDefault(TKey,TValue)) · [`ContainsKey`](#Std.Collections.Dictionary%3CTKey,TValue%3E.ContainsKey(TKey)) · [`Remove`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Remove(TKey)) · [`Clear`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Clear()) · [`Keys`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Keys()) · [`Values`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Values()) · [`Entries`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Entries()) · [`FindSlot`](#Std.Collections.Dictionary%3CTKey,TValue%3E.FindSlot(TKey)) · [`Grow`](#Std.Collections.Dictionary%3CTKey,TValue%3E.Grow())

<a id="Std.Collections.Dictionary<TKey,TValue>.Count"></a>
### int Count

[`src/libraries/z42.core/src/Collections/Dictionary.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L22)

元素个数（公开字段，可读可写）。

<a id="Std.Collections.Dictionary<TKey,TValue>.ctor()"></a>
### Dictionary()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L25)

新建空字典。

<a id="Std.Collections.Dictionary<TKey,TValue>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L37)

是否为空。

<a id="Std.Collections.Dictionary<TKey,TValue>.GetEnumerator()"></a>
### DictionaryEnumerator<TKey,TValue> GetEnumerator()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L40)

返回按插入顺序遍历的枚举器。

<a id="Std.Collections.Dictionary<TKey,TValue>.this[TKey]"></a>
### TValue this[TKey key] { get; set; }

[`src/libraries/z42.core/src/Collections/Dictionary.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L48)

读写 `key` 对应的值。getter 键不存在时返回 default(TValue)；setter 覆盖式写入， 覆盖不改变该键的插入位置。

<a id="Std.Collections.Dictionary<TKey,TValue>.Set(TKey,TValue)"></a>
### void Set(TKey key, TValue value)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L106)

覆盖式写入（等价索引器 setter）。

<a id="Std.Collections.Dictionary<TKey,TValue>.Get(TKey)"></a>
### TValue Get(TKey key)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L109)

取 `key` 对应的值；不存在返回 default(TValue)。

<a id="Std.Collections.Dictionary<TKey,TValue>.TryAdd(TKey,TValue)"></a>
### bool TryAdd(TKey key, TValue value)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L115)

仅当 key 不存在时插入（追加到末尾），返回是否成功插入（已存在则不覆盖、返回 false）。

<a id="Std.Collections.Dictionary<TKey,TValue>.GetValueOrDefault(TKey)"></a>
### TValue GetValueOrDefault(TKey key)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:124`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L124)

返回 key 对应的值，不存在则返回 default(TValue)。

<a id="Std.Collections.Dictionary<TKey,TValue>.GetValueOrDefault(TKey,TValue)"></a>
### TValue GetValueOrDefault(TKey key, TValue defaultValue)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L127)

返回 key 对应的值，不存在则返回调用方给定的 `defaultValue`。

<a id="Std.Collections.Dictionary<TKey,TValue>.ContainsKey(TKey)"></a>
### bool ContainsKey(TKey key)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L134)

是否含 key。

<a id="Std.Collections.Dictionary<TKey,TValue>.Remove(TKey)"></a>
### bool Remove(TKey key)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L155)

删除 key；删掉返回 true，键不存在返回 false。其余键相对顺序不变。

<a id="Std.Collections.Dictionary<TKey,TValue>.Clear()"></a>
### void Clear()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:170`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L170)

清空所有键值对，保留容量。

<a id="Std.Collections.Dictionary<TKey,TValue>.Keys()"></a>
### TKey[] Keys()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:178`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L178)

返回长度为 Count 的键快照数组（每次调用新建），按插入顺序。

<a id="Std.Collections.Dictionary<TKey,TValue>.Values()"></a>
### TValue[] Values()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:197`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L197)

返回长度为 Count 的值快照数组（每次调用新建），按插入顺序。

<a id="Std.Collections.Dictionary<TKey,TValue>.Entries()"></a>
### KeyValuePair<TKey,TValue>[] Entries()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:216`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L216)

返回键值对快照数组（每次调用新建），按插入顺序。

<a id="Std.Collections.Dictionary<TKey,TValue>.FindSlot(TKey)"></a>
### int FindSlot(TKey key)

[`src/libraries/z42.core/src/Collections/Dictionary.z42:234`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L234)

返回 key 在索引表中的槽。

若 key 存在，该槽指向它的 entry；否则是探测序列上的第一个空槽（即新键应放入
的槽）。容量 / 槽位管理入口，正常使用无需调用。

<a id="Std.Collections.Dictionary<TKey,TValue>.Grow()"></a>
### void Grow()

[`src/libraries/z42.core/src/Collections/Dictionary.z42:254`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Collections/Dictionary.z42#L254)

entries 容量翻倍（索引表随之翻倍）。容量管理入口，正常使用无需调用。

